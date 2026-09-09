//! Agent loop integration tests (SPEC Part B §3.7, note 07) — StubClient-driven
//! sans-IO runs. tempdirs only (course rule 4). The agent is PROPOSE-ONLY (D-01):
//! tools record intent, never execute — no `execute::` calls anywhere in this suite.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use disk_janitor::agent::{
    self, ChatClient, ModelReply, Msg, Role, RunSnapshot, RunState, StopReason, ToolCall, ToolDef,
    ToolMsg, ToolOutput, Usage,
};
use disk_janitor::config::{Budget, ModelPrice};
use disk_janitor::ledger::{self, Kind, LedgerRecord};
use disk_janitor::{Candidate, Evidence, EvidenceKind, Grade, Strength, VolumeInfo};
use serde_json::json;

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// Small token usage so default token/cost budgets never trip in scripted runs.
const USAGE: Usage = Usage {
    prompt_tokens: 40,
    completion_tokens: 10,
    cached_tokens: 0,
};

// --- scripted model client ---------------------------------------------------------

type Script = VecDeque<Box<dyn Fn(&[Msg]) -> ModelReply>>;

/// Scripted ChatClient: each `complete` pops the next reply producer. `&self` works
/// via interior mutability so the trait's `&self` signature stays honest.
struct StubClient {
    script: RefCell<Script>,
}

impl StubClient {
    fn new(script: Vec<Box<dyn Fn(&[Msg]) -> ModelReply>>) -> Self {
        Self {
            script: RefCell::new(script.into()),
        }
    }

    fn len(&self) -> usize {
        self.script.borrow().len()
    }
}

impl ChatClient for StubClient {
    fn complete(&self, msgs: &[Msg], _tools: &[ToolDef]) -> Result<ModelReply, agent::AgentError> {
        let f = self
            .script
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| agent::AgentError::Model("script exhausted".into()))?;
        Ok(f(msgs))
    }
}

fn tool_call(name: &str, id: &str, args: serde_json::Value) -> ModelReply {
    ModelReply {
        content: None,
        tool_calls: vec![ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: args,
        }],
        usage: USAGE,
    }
}

fn final_text(text: &str) -> ModelReply {
    ModelReply {
        content: Some(text.to_string()),
        tool_calls: Vec::new(),
        usage: USAGE,
    }
}

/// The field of the most recent tool result matching `field` (e.g. plan_id echoed
/// from a PlanBuilt result) — lets scripts reference ids the run just produced.
fn last_tool_field(msgs: &[Msg], field: &str) -> String {
    for m in msgs.iter().rev() {
        if m.role == Role::Tool {
            let v: serde_json::Value = serde_json::from_str(&m.content).expect("tool JSON");
            if let Some(s) = v.get(field).and_then(|x| x.as_str()) {
                return s.to_string();
            }
        }
    }
    panic!("no tool result carrying field {field}");
}

/// Drive the sans-IO loop to Done; collects every ToolMsg the run emitted.
fn drive(
    run: &mut agent::AgentRun,
    client: &dyn ChatClient,
    ctx: &mut agent::SessionCtx,
) -> (RunState, Vec<ToolMsg>) {
    let mut all = Vec::new();
    loop {
        match run.next_step() {
            RunState::CallModel { .. } => {
                let reply = client
                    .complete(run.history(), &agent::tool_defs())
                    .expect("stub");
                run.feed_reply(reply);
            }
            RunState::CallTools { calls } => all.extend(run.execute_tools(&calls, ctx)),
            done @ RunState::Done { .. } => return (done, all),
        }
    }
}

// --- fixtures -----------------------------------------------------------------------

fn price() -> ModelPrice {
    ModelPrice {
        model: "test-model".into(),
        prompt_miss_per_1m: 0.27,
        prompt_hit_per_1m: 0.07,
        completion_per_1m: 1.10,
    }
}

fn cand(id: &str, path: &str, grade: Grade, bytes: u64, dev: u64, ino: u64) -> Candidate {
    Candidate {
        id: id.to_string(),
        path: Path::new(path).to_path_buf(),
        path_hash: disk_janitor::path_hash(Path::new(path)),
        dev,
        ino,
        alloc_bytes: bytes,
        app_bytes: bytes,
        grade,
        evidence: Vec::new(),
        regen: None,
        // UNIX_EPOCH = maximally cold → Old bucket; never refused for mtime.
        mtime: SystemTime::UNIX_EPOCH,
        nlink: 1,
        clone_id: None,
        locked: false,
    }
}

fn vol(dev: u64) -> VolumeInfo {
    VolumeInfo {
        dev,
        mount: Path::new("/").to_path_buf(),
        fs: "apfs".to_string(),
        total: 1 << 40,
        free_raw: 1 << 30,
        free_important: 1 << 30,
        free_opportunistic: 1 << 30,
        read_only: false,
        rename_excl: true,
    }
}

fn sess_ctx(home: &Path, candidates: Vec<Candidate>, vols: Vec<VolumeInfo>) -> agent::SessionCtx {
    agent::SessionCtx {
        candidates,
        vols,
        ledger: ledger::open(home).expect("ledger open"),
        files_scanned: 12_345,
        last_plan_id: None,
        last_plan_bytes: None,
    }
}

/// Three recoverable candidates (dev 1 = the one known volume), size-desc order.
fn default_candidates() -> Vec<Candidate> {
    vec![
        cand(
            "C-001",
            "/tmp/agent-test/a",
            Grade::PureCache,
            2 * GIB,
            1,
            101,
        ),
        cand(
            "C-002",
            "/tmp/agent-test/b",
            Grade::RebuildableFromSource,
            1 * GIB,
            1,
            102,
        ),
        cand(
            "C-003",
            "/tmp/agent-test/c",
            Grade::Expensive,
            500 * MIB,
            1,
            103,
        ),
    ]
}

fn budget_recs(home: &Path) -> Vec<LedgerRecord> {
    let (recs, _) = ledger::replay(&home.join("ledger.jsonl")).expect("replay");
    recs.into_iter()
        .filter(|r| r.kind == Kind::Budget)
        .map(|r| r.rec)
        .collect()
}

// --- happy path ---------------------------------------------------------------------

/// User intent → scan_summary → candidates → build_plan → record_verdict → Done.
/// Proposal-only: apply/restore never run here, and no tool result executes anything.
#[test]
fn happy_path_reaches_verdict() {
    let home = tempfile::tempdir().unwrap();
    let mut run =
        agent::AgentRun::new("腾出 25 GB", "test-model", vec![price()], Budget::default());
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);

    let client = StubClient::new(vec![
        Box::new(|_| tool_call("scan_summary", "call-1", json!({}))),
        Box::new(|_| tool_call("candidates", "call-2", json!({}))),
        Box::new(|_| tool_call("build_plan", "call-3", json!({}))),
        Box::new(|_| {
            tool_call(
                "record_verdict",
                "call-4",
                json!({"verdict": "proceed", "reason": "target met"}),
            )
        }),
    ]);

    let (done, msgs) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::RequestedByTool
        }
    );
    assert_eq!(client.len(), 0, "script fully consumed");

    // Trajectory shape: System, User, then four assistant/tool pairs.
    let roles: Vec<Role> = run.history().iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::System,
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Assistant,
            Role::Tool,
            Role::Assistant,
            Role::Tool,
            Role::Assistant,
            Role::Tool,
        ]
    );

    // Every tool result crossed the chokepoint: JSON, path-free, tagged to its call.
    assert_eq!(msgs.len(), 4);
    for (m, want_id) in msgs.iter().zip(["call-1", "call-2", "call-3", "call-4"]) {
        assert_eq!(m.tool_call_id, want_id);
        let v: serde_json::Value = serde_json::from_str(&m.content).expect("chokepoint JSON");
        assert!(v.is_object(), "tagged output expected, got {}", m.content);
        assert!(agent::leaks_path_identifiers(&m.content).is_none());
    }
    let scan: serde_json::Value = serde_json::from_str(&msgs[0].content).unwrap();
    assert_eq!(scan["t"], "scan_summary");
    assert_eq!(scan["volumes"], 1);
    assert_eq!(scan["candidates"], 3);
    assert_eq!(scan["bytes"], 3 * GIB + 500 * MIB);
    let cands: serde_json::Value = serde_json::from_str(&msgs[1].content).unwrap();
    assert_eq!(cands["items"][0]["id"], "C-001");
    assert_eq!(cands["items"][0]["bytes"], 2 * GIB);
    assert_eq!(cands["items"][0]["dir_shape"], "build-tree");
    let plan: serde_json::Value = serde_json::from_str(&msgs[2].content).unwrap();
    assert_eq!(plan["t"], "plan_built");
    assert!(plan["plan_id"].as_str().unwrap().starts_with("pl-") || plan["plan_id"].is_string());
    let verdict: serde_json::Value = serde_json::from_str(&msgs[3].content).unwrap();
    assert_eq!(verdict["verdict"], "proceed");

    // Budget record: exactly one, cumulative for the run (4 calls).
    run.finish(&mut ctx).unwrap();
    let recs = budget_recs(home.path());
    assert_eq!(recs.len(), 1);
    match &recs[0] {
        LedgerRecord::Budget {
            model,
            calls,
            prompt_tokens,
            completion_tokens,
            cost_usd,
        } => {
            assert_eq!(model, "test-model");
            assert_eq!(*calls, 4);
            assert_eq!(*prompt_tokens, 160);
            assert_eq!(*completion_tokens, 40);
            assert!(*cost_usd > 0.0);
        }
        other => panic!("expected Budget rec, got {other:?}"),
    }
    // finish is idempotent: a second call appends nothing.
    run.finish(&mut ctx).unwrap();
    assert_eq!(budget_recs(home.path()).len(), 1);
}

/// apply_plan/restore_op are PROPOSE-ONLY (D-01): they return awaiting=human-confirm
/// and nothing executes.
#[test]
fn apply_and_restore_are_propose_only() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new(
        "plan then stop",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);

    let client = StubClient::new(vec![
        Box::new(|_| tool_call("build_plan", "call-1", json!({"target_bytes": 2 * GIB}))),
        Box::new(|msgs| {
            let plan_id = last_tool_field(msgs, "plan_id");
            tool_call("apply_plan", "call-2", json!({"plan_id": plan_id}))
        }),
        Box::new(|_| tool_call("restore_op", "call-3", json!({"op_id": "op-1"}))),
        Box::new(|_| tool_call("record_verdict", "call-4", json!({"verdict": "proceed"}))),
    ]);

    let (done, msgs) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::RequestedByTool
        }
    );

    // apply_plan echoed the real plan_id back and proposed, never executed.
    let plan_id = last_tool_field(run.history(), "plan_id");
    let applied: serde_json::Value = serde_json::from_str(&msgs[1].content).unwrap();
    assert_eq!(applied["t"], "proposed");
    assert_eq!(applied["plan_id"], plan_id);
    assert_eq!(applied["awaiting"], "human-confirm");
    let restored: serde_json::Value = serde_json::from_str(&msgs[2].content).unwrap();
    assert_eq!(restored["t"], "restore_proposed");
    assert_eq!(restored["op_id"], "op-1");
    assert_eq!(restored["awaiting"], "human-confirm");

    // No quarantine path, no execution residue anywhere in the trajectory.
    let all: String = run
        .history()
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!all.contains("quarantine_path"));
    assert!(!all.contains("/tmp/agent-test"), "path leaked: {all}");
}

// --- path-blindness -----------------------------------------------------------------

/// Real tempdir paths + name hints seeded into candidates and evidence; after the run
/// EVERY history message passes the leak set.
#[test]
fn history_is_path_blind_with_real_paths() {
    let td = tempfile::tempdir().unwrap();
    let app_dir = td.path().join("My App.app");
    fs::create_dir_all(app_dir.join("Contents/MacOS")).unwrap();
    fs::write(
        app_dir.join("Contents/MacOS/binary"),
        vec![0u8; (4 * MIB) as usize],
    )
    .unwrap();
    let nm_dir = td.path().join("src/node_modules/pkg");
    fs::create_dir_all(&nm_dir).unwrap();
    fs::write(nm_dir.join("index.js"), b"x".repeat(1024)).unwrap();

    let mut c1 = cand(
        "C-001",
        app_dir.to_str().unwrap(),
        Grade::Redownloadable,
        4 * MIB,
        1,
        101,
    );
    c1.mtime = SystemTime::now() - Duration::from_secs(3 * 86_400); // week bucket
    c1.evidence = vec![Evidence {
        kind: EvidenceKind::BuildArtifactShape,
        detail: format!("{}", app_dir.display()),
        strength: Strength::Strong,
    }];
    let mut c2 = cand(
        "C-002",
        nm_dir.to_str().unwrap(),
        Grade::PureCache,
        1 * MIB,
        1,
        102,
    );
    c2.mtime = SystemTime::now() - Duration::from_secs(90 * 86_400); // old bucket
    c2.evidence = vec![Evidence {
        kind: EvidenceKind::OrphanTree,
        detail: format!("{}", nm_dir.display()),
        strength: Strength::Strong,
    }];

    let mut run = agent::AgentRun::new(
        "what can go",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let ledger_home = tempfile::tempdir().unwrap();
    let mut ctx = sess_ctx(ledger_home.path(), vec![c1, c2], vec![vol(1)]);

    let client = StubClient::new(vec![
        Box::new(|_| tool_call("scan_summary", "call-1", json!({}))),
        Box::new(|_| tool_call("candidates", "call-2", json!({}))),
        Box::new(|_| final_text("done reviewing")),
    ]);

    let (done, _) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::ModelStop
        }
    );

    // EVERY message — system, user, assistant, tool results — passes the leak set.
    assert!(!run.history().is_empty());
    for m in run.history() {
        assert!(
            agent::leaks_path_identifiers(&m.content).is_none(),
            "leak in {:?} message: {}",
            m.role,
            m.content
        );
    }
    // And the candidates result specifically carried no tempdir/name residue.
    let cands: serde_json::Value =
        serde_json::from_str(&run.history()[5].content).expect("tool result");
    let blob = serde_json::to_string(&cands).unwrap();
    assert!(!blob.contains(&td.path().display().to_string()));
    assert!(!blob.contains("My App"), "app name leaked: {blob}");
    assert!(!blob.contains("node_modules"), "name hint leaked: {blob}");
    assert!(
        blob.contains("BuildArtifactShape"),
        "evidence text lost: {blob}"
    );
    assert_eq!(cands["items"][0]["mtime_bucket"], "week");
    assert_eq!(cands["items"][1]["mtime_bucket"], "old");
}

// --- budget & stop conditions -------------------------------------------------------

/// 0.5 USD per call, cap 1.0: calls 1+2 run (cum 1.0), the third call is interrupted →
/// Done{CostBudget}; the ledger holds exactly one Budget rec with cost ≈ 1.0.
#[test]
fn cost_budget_interrupts_before_third_call() {
    let home = tempfile::tempdir().unwrap();
    let price = ModelPrice {
        model: "test-model".into(),
        prompt_miss_per_1m: 1.0,
        prompt_hit_per_1m: 0.0,
        completion_per_1m: 0.0,
    };
    let usage = Usage {
        prompt_tokens: 500_000,
        completion_tokens: 0,
        cached_tokens: 0,
    };
    let cap = Budget {
        cost_usd: 1.0,
        max_turns: 12,
        tokens: 0,
    }; // tokens 0 = unlimited

    let mut run = agent::AgentRun::new("budget test", "test-model", vec![price], cap);
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let client = StubClient::new(vec![
        Box::new(move |_| ModelReply {
            content: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "candidates".into(),
                arguments: json!({}),
            }],
            usage,
        }),
        Box::new(move |_| ModelReply {
            content: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "candidates".into(),
                arguments: json!({}),
            }],
            usage,
        }),
        Box::new(move |_| ModelReply {
            content: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "candidates".into(),
                arguments: json!({}),
            }],
            usage,
        }),
    ]);

    let (done, _) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::CostBudget
        }
    );
    assert_eq!(
        client.len(),
        1,
        "third reply never consumed — model was not called"
    );

    run.finish(&mut ctx).unwrap();
    let recs = budget_recs(home.path());
    assert_eq!(recs.len(), 1, "exactly one Budget record");
    match &recs[0] {
        LedgerRecord::Budget {
            model,
            calls,
            prompt_tokens,
            completion_tokens,
            cost_usd,
        } => {
            assert_eq!(model, "test-model");
            assert_eq!(*calls, 2);
            assert_eq!(*prompt_tokens, 1_000_000);
            assert_eq!(*completion_tokens, 0);
            assert!((cost_usd - 1.0).abs() < 1e-9, "cost was {cost_usd}");
        }
        other => panic!("expected Budget rec, got {other:?}"),
    }
}

/// max_turns = 2 → exactly two model calls, then Done{MaxTurns}.
#[test]
fn max_turns_stops_the_loop() {
    let home = tempfile::tempdir().unwrap();
    let cap = Budget {
        cost_usd: 0.0,
        max_turns: 2,
        tokens: 0,
    }; // only the turn cap
    let mut run = agent::AgentRun::new("turns test", "test-model", vec![price()], cap);
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let client = StubClient::new(vec![
        Box::new(|_| tool_call("candidates", "c1", json!({}))),
        Box::new(|_| tool_call("candidates", "c2", json!({}))),
        Box::new(|_| tool_call("candidates", "c3", json!({}))),
    ]);

    let (done, _) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::MaxTurns
        }
    );
    assert_eq!(client.len(), 1, "third reply never consumed");
    assert_eq!(run.budget().turns, 2);
}

/// The same (name, args) call failing TOOL_RETRY_LIMIT times → Done{ToolFailLoop}.
#[test]
fn tool_retry_cap_stops_a_stuck_tool() {
    let home = tempfile::tempdir().unwrap();
    let mut run =
        agent::AgentRun::new("retry test", "test-model", vec![price()], Budget::default());
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let bad = json!({"filter": {"grade": "no_such_grade"}});
    let b1 = bad.clone();
    let b2 = bad.clone();
    let b3 = bad.clone();
    let client = StubClient::new(vec![
        Box::new(move |_| tool_call("candidates", "c1", b1.clone())),
        Box::new(move |_| tool_call("candidates", "c2", b2.clone())),
        Box::new(move |_| tool_call("candidates", "c3", b3.clone())),
    ]);

    let (done, msgs) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::ToolFailLoop
        }
    );
    assert_eq!(msgs.len(), 3, "three failed attempts recorded");
    for m in &msgs {
        let v: serde_json::Value = serde_json::from_str(&m.content).unwrap();
        assert_eq!(v["t"], "error");
    }
    assert_eq!(
        client.len(),
        0,
        "loop stopped exactly at the 3rd failure — no 4th call"
    );
}

// --- snapshot round-trip ------------------------------------------------------------

/// RunSnapshot survives serde_json and resumes to the same Done state.
#[test]
fn snapshot_round_trip_resumes() {
    let home = tempfile::tempdir().unwrap();
    let run = agent::AgentRun::new(
        "snapshot test",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let snap = run.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let restored: RunSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.state, RunState::CallModel { turn: 1 });
    assert_eq!(restored.history.len(), 2, "system + user present");

    let mut run2 = restored.resume();
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let client = StubClient::new(vec![
        Box::new(|_| tool_call("scan_summary", "call-1", json!({}))),
        Box::new(|_| tool_call("record_verdict", "call-2", json!({"verdict": "ok"}))),
    ]);
    let (done, _) = drive(&mut run2, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::RequestedByTool
        }
    );
    assert_eq!(
        run2.history().len(),
        6,
        "resumed run continued from snapshot"
    );
    assert_eq!(run2.history()[0].role, Role::System);
    assert_eq!(run2.history()[1].content, "snapshot test");
}

/// Ctrl-C path: cancel() → Done{UserCancel}, and finish() still posts the budget rec.
#[test]
fn cancel_stops_with_user_cancel() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new(
        "cancel test",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    run.cancel();
    assert_eq!(
        run.state(),
        &RunState::Done {
            reason: StopReason::UserCancel
        }
    );
    run.finish(&mut ctx).unwrap();
    assert_eq!(budget_recs(home.path()).len(), 1);
}

// --- chokepoint ---------------------------------------------------------------------

/// A path smuggled into candidate evidence detail does not survive serialization.
#[test]
fn chokepoint_scrubs_smuggled_path() {
    let path = "/Users/aaikyn1/Code/Secret Cache.app";
    let out = ToolOutput::Candidates {
        items: vec![agent::CandDto {
            id: "C-001".into(),
            bytes: 1 << 30,
            grade: Grade::PureCache,
            evidence: vec![format!("orphan_tree: {path}")],
            mtime_bucket: agent::MtimeBucket::Old,
            dir_shape: "build-tree".into(),
        }],
    };
    let json = agent::serialize_tool_result(&out);
    assert!(
        !json.contains("Secret Cache"),
        "smuggled path survived: {json}"
    );
    assert!(agent::leaks_path_identifiers(&json).is_none());
    assert!(json.contains("orphan_tree"));
}

/// The §3.7 cost formula: hit/miss/completion split per 1M.
#[test]
fn cost_of_splits_hit_miss_completion() {
    let p = ModelPrice {
        model: "m".into(),
        prompt_miss_per_1m: 0.27,
        prompt_hit_per_1m: 0.07,
        completion_per_1m: 1.10,
    };
    let u = Usage {
        prompt_tokens: 2_000_000,
        completion_tokens: 1_000_000,
        cached_tokens: 1_000_000,
    };
    // (2M − 1M)·0.27 + 1M·0.07 + 1M·1.10, per 1M = 0.27 + 0.07 + 1.10 = 1.44
    assert!((agent::cost_of(&u, &p) - 1.44).abs() < 1e-9);
}
