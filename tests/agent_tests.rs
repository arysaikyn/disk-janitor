//! Agent loop integration tests (SPEC Part B §3.7, note 07) — StubClient-driven
//! sans-IO runs. tempdirs only (course rule 4). The agent is PROPOSE-ONLY (D-01):
//! tools record intent, never execute — no `execute::` calls anywhere in this suite.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use disk_janitor::agent::{
    self, ChatClient, ModelReply, Msg, ProgressEvent, ProgressPhase, Role, RunSnapshot, RunState,
    StopReason, ToolCall, ToolDef, ToolMsg, ToolOutput, Usage,
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

fn verdict_recs(home: &Path) -> Vec<LedgerRecord> {
    let (recs, _) = ledger::replay(&home.join("ledger.jsonl")).expect("replay");
    recs.into_iter()
        .filter(|r| r.kind == Kind::Verdict)
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

    // E4: record_verdict persists exactly ONE run-level Verdict rec, model-tagged.
    let verdicts = verdict_recs(home.path());
    assert_eq!(verdicts.len(), 1);
    match &verdicts[0] {
        LedgerRecord::Verdict {
            verdict,
            reason,
            model,
        } => {
            assert_eq!(verdict, "proceed");
            assert_eq!(reason.as_deref(), Some("target met"));
            assert_eq!(model, "test-model");
        }
        other => panic!("expected Verdict rec, got {other:?}"),
    }

    // finish is idempotent: a second call appends nothing (budget OR verdict).
    run.finish(&mut ctx).unwrap();
    assert_eq!(budget_recs(home.path()).len(), 1);
    assert_eq!(verdict_recs(home.path()).len(), 1);
}

/// E4: a natural-stop run (final text, NO record_verdict call) posts the Budget rec
/// but NO Verdict rec — the verdict only exists when the model actually recorded one.
#[test]
fn natural_stop_writes_no_verdict_rec() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new(
        "natural stop",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let client = StubClient::new(vec![Box::new(|_| final_text("nothing to reclaim"))]);

    let (done, _) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::ModelStop
        }
    );
    run.finish(&mut ctx).unwrap();
    assert_eq!(
        verdict_recs(home.path()).len(),
        0,
        "no record_verdict call → no Verdict rec"
    );
    assert_eq!(budget_recs(home.path()).len(), 1, "Budget rec still posted");
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

// --- ask-before-act permission policy (SPEC §9 row 50) ------------------------------

/// Fail-closed: a tool name outside the registry is denied, never executed. The denial
/// is typed apart from a validation `Error`, and the model-influenced name it echoes
/// still crosses the chokepoint (the path is scrubbed from the tool field).
#[test]
fn unregistered_tool_is_denied_not_errored() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new("deny", "test-model", vec![price()], Budget::default());
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);

    let hostile = "rm_rf /Users/aaikyn1/node_modules";
    let client = StubClient::new(vec![
        Box::new(move |_| tool_call(hostile, "call-1", json!({}))),
        Box::new(|_| final_text("acknowledged")),
    ]);

    let (done, msgs) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::ModelStop
        }
    );

    let denied: serde_json::Value = serde_json::from_str(&msgs[0].content).unwrap();
    assert_eq!(
        denied["t"], "denied",
        "a policy denial must not read as an error"
    );
    assert_eq!(
        denied["tool"], "rm_rf [redacted]",
        "denied tool not scrubbed"
    );
    assert!(
        !denied["reason"].as_str().unwrap().is_empty(),
        "denial carries no reason"
    );
    assert!(!msgs[0].content.contains("/Users/aaikyn1"), "path leaked");
}

/// A `Deny` is permanent, so it must not consume the retry budget: four consecutive
/// denials — one past `TOOL_RETRY_LIMIT` — leave the run alive to finish normally
/// instead of tripping `ToolFailLoop`.
#[test]
fn repeated_denial_never_trips_the_tool_fail_loop() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new("retry", "test-model", vec![price()], Budget::default());
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);

    let mut script: Vec<Box<dyn Fn(&[Msg]) -> ModelReply>> = Vec::new();
    for i in 0..4 {
        script.push(Box::new(move |_| {
            tool_call("exfiltrate", &format!("call-{i}"), json!({}))
        }));
    }
    script.push(Box::new(|_| final_text("no other route")));
    let client = StubClient::new(script);

    let (done, msgs) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::ModelStop
        },
        "denials must not count toward the tool fail loop"
    );
    assert_eq!(msgs.len(), 4);
    for m in &msgs {
        let v: serde_json::Value = serde_json::from_str(&m.content).unwrap();
        assert_eq!(v["t"], "denied");
    }
}

/// The policy routes each call: an `Auto` tool dispatches and reports, an `Ask` tool
/// reaches its propose-only branch, and a tool that fails on its own state stays a
/// validation `Error` rather than being relabelled as a policy denial.
#[test]
fn permission_policy_routes_auto_and_ask_calls() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new("policy", "test-model", vec![price()], Budget::default());
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);

    let client = StubClient::new(vec![
        Box::new(|_| tool_call("scan_summary", "call-1", json!({}))),
        Box::new(|_| tool_call("apply_plan", "call-2", json!({"plan_id": "pl-missing"}))),
        Box::new(|_| tool_call("build_plan", "call-3", json!({"target_bytes": 2 * GIB}))),
        Box::new(|msgs| {
            let plan_id = last_tool_field(msgs, "plan_id");
            tool_call("apply_plan", "call-4", json!({"plan_id": plan_id}))
        }),
        Box::new(|_| tool_call("record_verdict", "call-5", json!({"verdict": "proceed"}))),
    ]);

    let (done, msgs) = drive(&mut run, &client, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::RequestedByTool
        }
    );

    let scanned: serde_json::Value = serde_json::from_str(&msgs[0].content).unwrap();
    assert_eq!(scanned["t"], "scan_summary", "Auto tool was not dispatched");

    let bad: serde_json::Value = serde_json::from_str(&msgs[1].content).unwrap();
    assert_eq!(bad["t"], "error", "argument failure must stay an error");

    let applied: serde_json::Value = serde_json::from_str(&msgs[3].content).unwrap();
    assert_eq!(applied["t"], "proposed", "Ask tool must only propose");
    assert_eq!(applied["awaiting"], "human-confirm");
}

/// The permission table and the advertised registry must agree: a tool the loop offers
/// the model is never one the policy would always refuse.
#[test]
fn advertised_tools_are_never_denied() {
    for def in agent::tool_defs() {
        assert_ne!(
            agent::AgentPermission::for_tool(&def.name),
            agent::AgentPermission::Deny,
            "advertised tool {} is denied by policy",
            def.name
        );
    }
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

// --- R4: live progress streaming + interrupt -----------------------------------------

/// Collect the run's ProgressEvents through a callback (the exact wiring the
/// CLI/GUI drivers use), so sequence/turn/budget assertions stay readable.
fn collect_progress(
    run: &mut agent::AgentRun,
) -> std::sync::Arc<std::sync::Mutex<Vec<ProgressEvent>>> {
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&events);
    run.on_progress(Some(Box::new(move |ev: &ProgressEvent| {
        seen.lock().unwrap().push(ev.clone());
    })));
    events
}

fn phase_names(events: &[ProgressEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            serde_json::to_value(e.phase)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// R4: `on_progress` fires model_call → tool_exec → model_call → done in order,
/// carrying the running turn and cumulative budget; the wire shape serializes to
/// the snake_case tags the GUI consumes (`"phase":"model_call"`, nested budget).
#[test]
fn progress_events_stream_model_tool_done() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new(
        "progress test",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let client = StubClient::new(vec![
        Box::new(|_| tool_call("scan_summary", "t1", json!({}))),
        Box::new(|_| final_text("done")),
    ]);
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let events = collect_progress(&mut run);

    loop {
        match run.step(&client).expect("stub never errors") {
            RunState::CallTools { calls } => {
                run.execute_tools(&calls, &mut ctx);
            }
            RunState::Done { .. } => break,
            RunState::CallModel { .. } => unreachable!("step never returns CallModel"),
        }
    }
    run.finish(&mut ctx).unwrap();

    let evs = events.lock().unwrap();
    assert_eq!(
        phase_names(&evs),
        ["model_call", "tool_exec", "model_call", "done"],
        "exact R4 event sequence"
    );

    // Turn 1 model call: before the call, so the budget is still zero.
    let m1 = &evs[0];
    assert_eq!(
        (m1.turn, m1.phase, m1.tool.as_deref()),
        (1, ProgressPhase::ModelCall, None)
    );
    assert_eq!((m1.budget.tokens, m1.budget.cost_usd), (0, 0.0));
    // Wire shape: snake_case phase tag + nested budget object.
    let wire = serde_json::to_value(m1).unwrap();
    assert_eq!(wire["phase"], "model_call");
    assert_eq!(wire["budget"]["tokens"], 0);

    // Tool exec: same turn, names the tool, budget already counts the first call.
    let t = &evs[1];
    assert_eq!((t.turn, t.phase), (1, ProgressPhase::ToolExec));
    assert_eq!(t.tool.as_deref(), Some("scan_summary"));
    assert!(t.detail.contains("scan_summary"));
    assert_eq!(t.budget.tokens, 50, "USAGE 40+10 folded in after the call");

    // Done: last turn, cumulative budget for both calls, stop reason in detail.
    let d = &evs[3];
    assert_eq!((d.turn, d.phase), (2, ProgressPhase::Done));
    assert!(d.detail.contains("ModelStop"), "detail: {}", d.detail);
    assert_eq!(d.budget.tokens, 100);
    let expected_cost = 2.0 * agent::cost_of(&USAGE, &price());
    assert!((d.budget.cost_usd - expected_cost).abs() < 1e-12);
}

/// R4: cancel() mid-run → Done{UserCancel} fires a `done` event, and the next
/// step() returns Done WITHOUT touching the model (the script stays unconsumed)
/// — the run stops before the next model call.
#[test]
fn cancel_mid_run_stops_before_next_model_call() {
    let home = tempfile::tempdir().unwrap();
    let mut run = agent::AgentRun::new(
        "cancel test",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let client = StubClient::new(vec![
        Box::new(|_| tool_call("scan_summary", "t1", json!({}))),
        Box::new(|_| final_text("never called")),
    ]);
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let events = collect_progress(&mut run);

    // Turn 1: one model call + one tool execution, then cancel between turns.
    let st = run.step(&client).expect("stub");
    let RunState::CallTools { calls } = st else {
        panic!("expected CallTools after turn 1, got {st:?}");
    };
    run.execute_tools(&calls, &mut ctx);
    assert_eq!(client.len(), 1, "one reply remains before cancel");
    run.cancel();
    assert_eq!(
        run.state(),
        &RunState::Done {
            reason: StopReason::UserCancel
        }
    );

    // The next step must NOT consume the remaining script reply.
    let st = run.step(&client).expect("stub");
    assert!(matches!(st, RunState::Done { .. }));
    assert_eq!(
        run.state(),
        &RunState::Done {
            reason: StopReason::UserCancel
        }
    );
    assert_eq!(
        client.len(),
        1,
        "cancelled run must not call the model again"
    );

    // Exactly model_call → tool_exec → done(user_cancel), budget = one turn.
    let evs = events.lock().unwrap();
    assert_eq!(phase_names(&evs), ["model_call", "tool_exec", "done"]);
    let d = evs.last().unwrap();
    assert_eq!(d.phase, ProgressPhase::Done);
    assert!(d.detail.contains("UserCancel"), "detail: {}", d.detail);
    assert_eq!(d.budget.tokens, 50, "one turn ran before cancel");
}

// --- R5: session save/load -----------------------------------------------------------

/// R5: persist_snapshot writes `<dir>/<run_id>.json` atomically (WAL protocol);
/// load_snapshot round-trips state/budget/history exactly, and the resumed run
/// continues the loop from the restored `RunState` (SPEC §3.7).
#[test]
fn persist_load_snapshot_round_trips_and_resumes() {
    let home = tempfile::tempdir().unwrap();
    let sess_dir = home.path().join("sessions");
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);

    // Drive the run through ONE model call + tool exec, leaving it mid-run
    // (CallModel{turn:2}) — the interesting resume point.
    let mut run = agent::AgentRun::new(
        "persist mid-run",
        "test-model",
        vec![price()],
        Budget::default(),
    );
    let client1 = StubClient::new(vec![Box::new(|_| {
        tool_call("scan_summary", "call-1", json!({}))
    })]);
    assert_eq!(run.next_step(), RunState::CallModel { turn: 1 });
    run.feed_reply(
        client1
            .complete(run.history(), &agent::tool_defs())
            .unwrap(),
    );
    let calls = match run.next_step() {
        RunState::CallTools { calls } => calls,
        s => panic!("expected CallTools, got {s:?}"),
    };
    run.execute_tools(&calls, &mut ctx);
    assert_eq!(run.state(), &RunState::CallModel { turn: 2 });

    // Persist → load → field-level round-trip equality (state, budget, history).
    // The file is the single source of truth: persist took its own snapshot.
    let path = run.persist_snapshot(&sess_dir).unwrap();
    assert_eq!(
        path.file_name().unwrap(),
        format!("{}.json", run.run_id()).as_str(),
        "file name is <run_id>.json"
    );
    let file: RunSnapshot = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(file.run_id, run.run_id().to_string());
    assert_eq!(
        file.state,
        *run.state(),
        "file state matches the run at persist time"
    );
    assert_eq!(file.history, run.history());
    assert_eq!(file.budget.turns, run.budget().turns);
    assert_eq!(file.budget.cost_usd, run.budget().cost_usd);

    let mut run2 = agent::load_snapshot(&path).unwrap();
    assert_eq!(run2.run_id(), run.run_id());
    assert_eq!(run2.state(), run.state());
    assert_eq!(run2.history(), run.history());
    assert_eq!(run2.budget().turns, file.budget.turns);
    assert_eq!(run2.budget().cost_usd, file.budget.cost_usd);
    assert_eq!(run2.budget().cap.max_turns, file.budget.cap.max_turns);

    // Resume continues: one more tool turn → record_verdict → Done, with the
    // pre-resume history intact.
    let client2 = StubClient::new(vec![Box::new(|_| {
        tool_call("record_verdict", "call-2", json!({"verdict": "ok"}))
    })]);
    let (done, _) = drive(&mut run2, &client2, &mut ctx);
    assert_eq!(
        done,
        RunState::Done {
            reason: StopReason::RequestedByTool
        }
    );
    assert_eq!(
        run2.history().len(),
        run.history().len() + 2,
        "resumed run continued from the restored state"
    );
    assert_eq!(run2.history()[0].role, Role::System);
    assert_eq!(run2.history()[1].content, "persist mid-run");
}

/// R5 wire contract: the persisted file carries exactly the keys the GUI
/// trajectory view renders (Ledger.svelte `Trajectory`/`TrajMsg` interfaces).
#[test]
fn snapshot_json_has_trajectory_fields() {
    let home = tempfile::tempdir().unwrap();
    let mut ctx = sess_ctx(home.path(), default_candidates(), vec![vol(1)]);
    let mut run =
        agent::AgentRun::new("shape test", "test-model", vec![price()], Budget::default());
    let client = StubClient::new(vec![
        Box::new(|_| tool_call("scan_summary", "call-1", json!({}))),
        Box::new(|_| tool_call("record_verdict", "call-2", json!({"verdict": "ok"}))),
    ]);
    drive(&mut run, &client, &mut ctx);
    let path = run.persist_snapshot(&home.path().join("sessions")).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for key in ["run_id", "saved_at", "model", "state", "budget", "history"] {
        assert!(v.get(key).is_some(), "missing snapshot key {key}");
    }
    assert!(
        v["saved_at"].as_str().is_some(),
        "saved_at must be an RFC 3339 string"
    );
    for key in ["turns", "prompt_tokens", "completion_tokens", "cost_usd"] {
        assert!(v["budget"].get(key).is_some(), "missing budget key {key}");
    }
    let h = v["history"].as_array().expect("history is an array");
    // Assistant messages carry the model's tool calls (name + parsed arguments).
    let assistant = h
        .iter()
        .find(|m| m["role"] == "assistant")
        .expect("assistant msg");
    let tc = assistant["tool_calls"]
        .as_array()
        .expect("assistant tool_calls");
    assert_eq!(tc[0]["name"], "scan_summary");
    assert!(tc[0]["arguments"].is_object(), "arguments is parsed JSON");
    // Tool messages carry the chokepoint-serialized result + the call id.
    let tool = h.iter().find(|m| m["role"] == "tool").expect("tool msg");
    assert!(
        tool["content"].as_str().is_some(),
        "tool result content is a string"
    );
    assert_eq!(tool["tool_call_id"], "call-1");
}
