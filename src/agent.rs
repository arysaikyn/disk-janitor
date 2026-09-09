//! `agent/` — tool-calling loop, the privacy chokepoint (SPEC Part B §3.7 + note 07).
//!
//! A sans-IO rig-run style state machine (`AgentRun`): steppable, serializable, testable
//! without a network. The model call is injected via the `ChatClient` trait; the HTTP
//! implementation (async-openai/reqwest/tokio) lands with the Cargo.toml deps — this
//! module only defines the protocol shape (`Msg`/`ToolCall`/`ToolDef`/`Usage`) plus the
//! driver loop helper `AgentRun::step`.
//!
//! PROPOSE-ONLY (D-01): tools record intent and return plan references; the ID→path map
//! is resolved inside `execute/` AFTER a human confirms. No tool in this module calls
//! `execute::` — `apply_plan`/`restore_op` return `awaiting: "human-confirm"`.
//!
//! Path-blind invariant (§5.4): the model sees only opaque ids (C-001…), byte counts,
//! grades, and evidence summaries. Enforcement = (1) tool outputs carry no paths,
//! (2) `serialize_tool_result` is the ONE serializer chokepoint — every string leaf is
//! scrubbed by `scrub_text` before it becomes LLM-visible JSON, (3) `leaks_path_identifiers`
//! is the canonical leak set used by the CI tests (module `leak_tests` + `tests/agent_tests.rs`).

use std::collections::HashMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::config::{Budget, ModelPrice};
use crate::ledger::{Kind, LedgerRecord};
use crate::plan::{self, PolicyConstraintsShape};
use crate::{Candidate, Grade, VolumeInfo};

/// Per-call retry cap per (tool name, args) hash (swiftide `tool_retry_limit=3`).
pub const TOOL_RETRY_LIMIT: u32 = 3;

// --- trajectory messages ---------------------------------------------------------

/// One chat message in the run trajectory. `tool_calls`/`tool_call_id` are wire-only
/// fields (the OpenAI tool-calling protocol requires echoing the assistant's calls
/// verbatim and tagging tool results with `tool_call_id`); `content` is the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Msg {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// One model-issued tool invocation. `arguments` is parsed JSON (the wire carries a JSON
/// *string*; the HTTP client deserializes it into this Value before entering the loop —
/// Ollama-style id-less calls get a synthetic id there too).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A tool result message, already serialized through the chokepoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMsg {
    pub tool_call_id: String,
    pub name: String,
    pub content: String,
}

/// Tool definition exposed to the model (generic names/descriptions only — no path or
/// name hints: e.g. never "delete a node_modules dir").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

// --- sans-IO state machine --------------------------------------------------------

/// Rig-run step shape: the loop alternates CallModel → CallTools → CallModel until Done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    /// The next model call, 1-based call index `turn` (turn = calls made so far + 1).
    CallModel {
        turn: u32,
    },
    /// Model replied with tool calls; the driver executes them via `execute_tools`.
    CallTools {
        calls: Vec<ToolCall>,
    },
    Done {
        reason: StopReason,
    },
}

/// Stop conditions (§3.7 table + note 07 §budget).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Natural stop: model produced final text with no tool calls.
    ModelStop,
    /// The `record_verdict` control tool was called (swiftide Stop pattern).
    RequestedByTool,
    MaxTurns,
    TokenBudget,
    CostBudget,
    /// Ctrl-C (WAL-consistent per SPEC §3.2) — set via `AgentRun::cancel`.
    UserCancel,
    /// Same (name, args) tool call failed `TOOL_RETRY_LIMIT` times.
    ToolFailLoop,
    ContextCap,
}

/// Cumulative usage/cost counters against the configured cap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetState {
    /// Model calls made (ledger `calls`).
    pub turns: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Cumulative total tokens (prompt + completion) — the interrupt gauge.
    pub tokens: u64,
    pub cost_usd: f64,
    /// The cap. A field of 0 means "unlimited" for that axis.
    pub cap: Budget,
}

impl BudgetState {
    pub fn new(cap: Budget) -> Self {
        Self {
            turns: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            tokens: 0,
            cost_usd: 0.0,
            cap,
        }
    }
}

// --- model interface (injected; HTTP impl comes with the Cargo.toml deps) ---------

/// Provider-reported usage — ground truth for billing (never a local tokenizer, note 07
/// §cost). Reasoning tokens are already inside `completion_tokens` — never double-count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_tokens: u64,
}

/// One model reply. `tool_calls` empty ⇒ natural stop (`Done{ModelStop}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelReply {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("model call failed: {0}")]
    Model(String),
    #[error("invalid agent state: {0}")]
    State(String),
}

/// The injected model client. `complete` receives the full trajectory and the tool
/// defs; it must return the model's reply (or a transport error). The sans-IO loop
/// never blocks on the network.
pub trait ChatClient {
    fn complete(&self, msgs: &[Msg], tools: &[ToolDef]) -> Result<ModelReply, AgentError>;
}

// --- tool outputs (redacted shapes) ------------------------------------------------

/// Tool results. Only `serialize_tool_result` turns these into LLM-visible JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToolOutput {
    ScanSummary {
        volumes: u32,
        files: u64,
        bytes: u64,
        candidates: u32,
        recoverable_bytes: u64,
    },
    Candidates {
        items: Vec<CandDto>,
    },
    PlanBuilt {
        plan_id: String,
        quarantine_entries: u32,
        quarantine_bytes: u64,
        refused: u32,
        summary: String,
    },
    /// apply_plan records intent only — the human confirms before `execute/` runs.
    Proposed {
        plan_id: String,
        bytes: u64,
        awaiting: String,
    },
    /// restore_op records intent only — the human confirms before `execute/` runs.
    RestoreProposed {
        op_id: String,
        awaiting: String,
    },
    Receipts {
        items: Vec<ReceiptDto>,
    },
    VerdictRecorded {
        verdict: String,
        reason: Option<String>,
    },
    /// Validation/execution failure — counted toward the per-call retry hash.
    Error {
        message: String,
    },
}

/// One candidate as the model sees it — NO paths, NO real dir shapes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandDto {
    /// Opaque id, "C-042" (deterministic size-desc order).
    pub id: String,
    pub bytes: u64,
    pub grade: Grade,
    /// Localized short evidence strings (scrubbed at the chokepoint).
    pub evidence: Vec<String>,
    pub mtime_bucket: MtimeBucket,
    /// Always the generic literal "build-tree" — a real shape name would leak.
    pub dir_shape: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtimeBucket {
    Hour,
    Day,
    Week,
    Old,
}

/// A receipt the model may query — id is an opaque id, never a path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptDto {
    pub id: String,
    pub verdict: String,
    pub bytes: u64,
    pub grade: Grade,
}

// --- session context ---------------------------------------------------------------

/// Cross-tool session state, seeded by the CALLER (the GUI/CLI runs the scan first;
/// M4 keeps `candidates` pre-seeded — "scan_summary starts an async scan" is M5+ wiring
/// and the scan tool here reports the seeded totals). `ledger` is the single writer
/// handle used by `AgentRun::finish`.
pub struct SessionCtx {
    pub candidates: Vec<Candidate>,
    pub vols: Vec<VolumeInfo>,
    pub ledger: crate::ledger::Store,
    /// Total files reported by the scan (caller-seeded).
    pub files_scanned: u64,
    /// Filled by `build_plan`; `apply_plan` only accepts this id.
    pub last_plan_id: Option<String>,
    pub last_plan_bytes: Option<u64>,
}

// --- the run -----------------------------------------------------------------------

/// sans-IO agent run (rig-run pattern). Drive with `step` (or manually:
/// `next_step` → if CallModel, `client.complete` + `feed_reply` → if CallTools,
/// `execute_tools` — until Done), then `finish` once to persist the budget record.
pub struct AgentRun {
    run_id: String,
    model: String,
    state: RunState,
    budget: BudgetState,
    history: Vec<Msg>,
    /// Session-scoped receipts (seeded from `ctx.candidates` on first ledger_receipts).
    receipts: Vec<ReceiptDto>,
    /// Per (name, args-hash) failure counts for the ToolFailLoop cap.
    tool_attempts: HashMap<u64, u32>,
    ledger_posted: bool,
    prices: Vec<ModelPrice>,
}

const SYSTEM_PROMPT: &str = "You are the reclaim agent of a disk-reclamation tool. \
You propose reclaim actions; a human always confirms before anything is executed. \
You see only opaque item ids, byte counts, grades, and evidence summaries — never paths. \
Tools: scan_summary, candidates, build_plan, apply_plan, restore_op, ledger_receipts, \
record_verdict. When you have reached a recommendation, call record_verdict and stop.";

impl AgentRun {
    /// Start a run from a user intent sentence (the NL policy input, passed verbatim —
    /// user-authored, note 07 §privacy 4).
    pub fn new(intent: &str, model: &str, prices: Vec<ModelPrice>, cap: Budget) -> Self {
        Self {
            run_id: uuid::Uuid::new_v4().to_string(),
            model: model.to_string(),
            state: RunState::CallModel { turn: 1 },
            budget: BudgetState::new(cap),
            history: vec![
                Msg {
                    role: Role::System,
                    content: SYSTEM_PROMPT.to_string(),
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                },
                Msg {
                    role: Role::User,
                    content: intent.to_string(),
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                },
            ],
            receipts: Vec::new(),
            tool_attempts: HashMap::new(),
            ledger_posted: false,
            prices,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn state(&self) -> &RunState {
        &self.state
    }

    pub fn history(&self) -> &[Msg] {
        &self.history
    }

    pub fn budget(&self) -> &BudgetState {
        &self.budget
    }

    /// Pure transition: applies budget/stop pre-checks at a CallModel boundary. Returns
    /// the current state (owned). CallTools and Done pass through unchanged.
    pub fn next_step(&mut self) -> RunState {
        if matches!(self.state, RunState::CallModel { .. })
            && let Some(reason) = self.budget_stop()
        {
            self.state = RunState::Done { reason };
        }
        self.state.clone()
    }

    /// Inject one model reply (the driver obtained it from `ChatClient::complete`).
    /// CallModel → CallTools when the reply has tool calls, → Done{ModelStop} otherwise.
    /// Usage is folded into the budget immediately; a budget stop beats any reply.
    pub fn feed_reply(&mut self, reply: ModelReply) -> RunState {
        if let Some(reason) = self.budget_stop() {
            self.state = RunState::Done { reason };
            return self.state.clone();
        }
        let turn = match self.state {
            RunState::CallModel { turn } => turn,
            _ => return self.state.clone(), // no-op outside CallModel
        };
        self.budget.turns = turn;
        self.budget.prompt_tokens += reply.usage.prompt_tokens;
        self.budget.completion_tokens += reply.usage.completion_tokens;
        self.budget.tokens += reply.usage.prompt_tokens + reply.usage.completion_tokens;
        self.budget.cost_usd += cost_of(&reply.usage, &self.price());

        self.history.push(Msg {
            role: Role::Assistant,
            content: reply.content.unwrap_or_default(),
            tool_calls: reply.tool_calls.clone(),
            tool_call_id: None,
        });

        self.state = if reply.tool_calls.is_empty() {
            RunState::Done {
                reason: StopReason::ModelStop,
            }
        } else {
            RunState::CallTools {
                calls: reply.tool_calls,
            }
        };
        self.state.clone()
    }

    /// Dispatch the model's tool calls against `ctx`. Every result crosses the
    /// `serialize_tool_result` chokepoint into history and the returned `ToolMsg`s.
    /// Transitions: → CallModel{turn+1}, or → Done{RequestedByTool} when the
    /// `record_verdict` control tool was called, or → Done{ToolFailLoop} when the same
    /// (name, args) call failed `TOOL_RETRY_LIMIT` times.
    pub fn execute_tools(&mut self, calls: &[ToolCall], ctx: &mut SessionCtx) -> Vec<ToolMsg> {
        if matches!(self.state, RunState::Done { .. }) {
            return Vec::new();
        }
        let mut msgs = Vec::with_capacity(calls.len());
        let mut stop_requested = false;
        for call in calls {
            let out = match call.name.as_str() {
                "scan_summary" => tool_scan_summary(ctx),
                "candidates" => tool_candidates(ctx, &call.arguments),
                "build_plan" => tool_build_plan(ctx, &call.arguments),
                "apply_plan" => tool_apply_plan(ctx, &call.arguments),
                "restore_op" => tool_restore_op(&call.arguments),
                "ledger_receipts" => tool_ledger_receipts(self, ctx, &call.arguments),
                "record_verdict" => {
                    stop_requested = true;
                    tool_record_verdict(&call.arguments)
                }
                other => ToolOutput::Error {
                    message: format!("unknown tool: {other}"),
                },
            };
            if matches!(out, ToolOutput::Error { .. }) {
                let n = self
                    .tool_attempts
                    .entry(retry_key(&call.name, &call.arguments))
                    .or_insert(0);
                *n += 1;
                if *n >= TOOL_RETRY_LIMIT {
                    self.state = RunState::Done {
                        reason: StopReason::ToolFailLoop,
                    };
                }
            } else {
                self.tool_attempts
                    .remove(&retry_key(&call.name, &call.arguments));
            }
            let content = serialize_tool_result(&out);
            self.history.push(Msg {
                role: Role::Tool,
                content: content.clone(),
                tool_calls: Vec::new(),
                tool_call_id: Some(call.id.clone()),
            });
            msgs.push(ToolMsg {
                tool_call_id: call.id.clone(),
                name: call.name.clone(),
                content,
            });
        }
        if !matches!(self.state, RunState::Done { .. }) {
            self.state = if stop_requested {
                RunState::Done {
                    reason: StopReason::RequestedByTool,
                }
            } else {
                RunState::CallModel {
                    turn: self.budget.turns + 1,
                }
            };
        }
        msgs
    }

    /// One full loop iteration: pure transition, then the model call if needed.
    /// Returns the resulting state (Done → the driver should call `finish`).
    pub fn step(&mut self, client: &dyn ChatClient) -> Result<RunState, AgentError> {
        match self.next_step() {
            RunState::CallModel { .. } => {
                let reply = client.complete(&self.history, &tool_defs())?;
                Ok(self.feed_reply(reply))
            }
            other => Ok(other),
        }
    }

    /// Ctrl-C path — marks the run Done{UserCancel} (WAL-consistent, SPEC §3.2).
    pub fn cancel(&mut self) {
        if !matches!(self.state, RunState::Done { .. }) {
            self.state = RunState::Done {
                reason: StopReason::UserCancel,
            };
        }
    }

    /// R5 session save: the serializable snapshot; persist via the caller
    /// (`~/.local/state/disk-janitor/sessions/<run_id>.json`), resume later.
    pub fn snapshot(&self) -> RunSnapshot {
        RunSnapshot {
            run_id: self.run_id.clone(),
            model: self.model.clone(),
            state: self.state.clone(),
            budget: self.budget.clone(),
            history: self.history.clone(),
            receipts: self.receipts.clone(),
            ledger_posted: self.ledger_posted,
            prices: self.prices.clone(),
        }
    }

    /// When the run has ended (any Done), append the run's verdict (if the
    /// `record_verdict` control tool was called) plus ONE cumulative Budget record
    /// to the ledger (D-20 model field). Idempotent: the run posts at most once.
    pub fn finish(&mut self, ctx: &mut SessionCtx) -> anyhow::Result<()> {
        if self.ledger_posted || !matches!(self.state, RunState::Done { .. }) {
            return Ok(());
        }
        // E4: `record_verdict` used to only echo into history — persist the verdict.
        if let Some((verdict, reason)) = self.recorded_verdict() {
            ctx.ledger.append(
                Kind::Verdict,
                &LedgerRecord::Verdict {
                    verdict,
                    reason,
                    model: self.model.clone(),
                },
            )?;
        }
        let rec = LedgerRecord::Budget {
            model: self.model.clone(),
            calls: self.budget.turns,
            prompt_tokens: self.budget.prompt_tokens,
            completion_tokens: self.budget.completion_tokens,
            cost_usd: self.budget.cost_usd,
        };
        ctx.ledger.append(Kind::Budget, &rec)?;
        self.ledger_posted = true;
        Ok(())
    }

    /// The last `record_verdict` tool result in the trajectory, if any:
    /// `(verdict, reason)`. Tool results live in history as serialized `ToolOutput`
    /// (`{"t":"verdict_recorded",…}`), so no extra run field is needed (E4).
    fn recorded_verdict(&self) -> Option<(String, Option<String>)> {
        self.history.iter().rev().find_map(|m| {
            if m.role != Role::Tool {
                return None;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&m.content) else {
                return None;
            };
            if v.get("t").and_then(|t| t.as_str()) != Some("verdict_recorded") {
                return None;
            }
            let verdict = v.get("verdict")?.as_str()?.to_string();
            let reason = v.get("reason").and_then(|r| r.as_str()).map(str::to_string);
            Some((verdict, reason))
        })
    }

    fn budget_stop(&self) -> Option<StopReason> {
        let b = &self.budget;
        let cap = &b.cap;
        if cap.max_turns > 0 && b.turns >= cap.max_turns {
            Some(StopReason::MaxTurns)
        } else if cap.tokens > 0 && b.tokens >= cap.tokens {
            Some(StopReason::TokenBudget)
        } else if cap.cost_usd > 0.0 && b.cost_usd >= cap.cost_usd {
            Some(StopReason::CostBudget)
        } else {
            None
        }
    }

    /// Price row for this run's model; falls back to the first row, then zero.
    fn price(&self) -> ModelPrice {
        self.prices
            .iter()
            .find(|p| p.model == self.model)
            .or_else(|| self.prices.first())
            .cloned()
            .unwrap_or(ModelPrice {
                model: "*".to_string(),
                prompt_miss_per_1m: 0.0,
                prompt_hit_per_1m: 0.0,
                completion_per_1m: 0.0,
            })
    }
}

/// R5 session save/load — `serde_json` round-trip + `resume`. Tool retry counters are
/// transient and reset on resume; everything else (state, budget, history) continues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub run_id: String,
    pub model: String,
    pub state: RunState,
    pub budget: BudgetState,
    pub history: Vec<Msg>,
    pub receipts: Vec<ReceiptDto>,
    pub ledger_posted: bool,
    pub prices: Vec<ModelPrice>,
}

impl RunSnapshot {
    pub fn resume(self) -> AgentRun {
        AgentRun {
            run_id: self.run_id,
            model: self.model,
            state: self.state,
            budget: self.budget,
            history: self.history,
            receipts: self.receipts,
            tool_attempts: HashMap::new(),
            ledger_posted: self.ledger_posted,
            prices: self.prices,
        }
    }
}

// --- cost accounting ----------------------------------------------------------------

/// Per-call cost in USD: `(prompt − cached)·p_miss + cached·p_hit + completion·p_completion`
/// per 1M tokens (§3.7; note 07 §cost). Provider-reported usage is ground truth;
/// reasoning tokens are already inside `completion_tokens` — never double-counted.
pub fn cost_of(u: &Usage, p: &ModelPrice) -> f64 {
    let miss = u.prompt_tokens.saturating_sub(u.cached_tokens) as f64;
    (miss * p.prompt_miss_per_1m
        + u.cached_tokens as f64 * p.prompt_hit_per_1m
        + u.completion_tokens as f64 * p.completion_per_1m)
        / 1_000_000.0
}

// --- tool registry ------------------------------------------------------------------

/// The 7 tool defs (SPEC §3.7: 6 tools + the `record_verdict` control tool). Names,
/// descriptions, and JSON-Schema descriptions carry no path or name hints.
pub fn tool_defs() -> Vec<ToolDef> {
    use serde_json::json;
    vec![
        ToolDef {
            name: "scan_summary".into(),
            description: "Report volume and candidate totals from the current scan. No item-level detail.".into(),
            parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
        },
        ToolDef {
            name: "candidates".into(),
            description: "List reclaimable items as opaque ids with byte counts, grades, and evidence summaries. Optional filters by grade, minimum bytes, and limit.".into(),
            parameters: json!({"type":"object","properties":{"filter":{"type":"object","properties":{"grade":{"type":"string"},"min_bytes":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1}},"additionalProperties":false}},"additionalProperties":false}),
        },
        ToolDef {
            name: "build_plan".into(),
            description: "Build a reclaim plan from the current candidate set under policy constraints. Returns counts and byte totals only.".into(),
            parameters: json!({"type":"object","properties":{"target_bytes":{"type":"integer","minimum":0},"min_bytes":{"type":"integer","minimum":0},"allow_grades":{"type":"array","items":{"type":"string"}}},"additionalProperties":false}),
        },
        ToolDef {
            name: "apply_plan".into(),
            description: "Propose applying a previously built plan. Nothing executes: a human must confirm.".into(),
            parameters: json!({"type":"object","properties":{"plan_id":{"type":"string"}},"required":["plan_id"],"additionalProperties":false}),
        },
        ToolDef {
            name: "restore_op".into(),
            description: "Propose restoring a previously recorded operation. Nothing executes: a human must confirm.".into(),
            parameters: json!({"type":"object","properties":{"op_id":{"type":"string"}},"required":["op_id"],"additionalProperties":false}),
        },
        ToolDef {
            name: "ledger_receipts".into(),
            description: "List recent item receipts with opaque ids, verdicts, byte counts, and grades.".into(),
            parameters: json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}),
        },
        ToolDef {
            name: "record_verdict".into(),
            description: "Record the final verdict for this run and stop. Call this when a recommendation has been reached.".into(),
            parameters: json!({"type":"object","properties":{"verdict":{"type":"string"},"reason":{"type":"string"}},"required":["verdict"],"additionalProperties":false}),
        },
    ]
}

// --- tool implementations (propose-only; ctx is caller-seeded) -----------------------

fn tool_scan_summary(ctx: &SessionCtx) -> ToolOutput {
    let bytes: u64 = ctx.candidates.iter().map(|c| c.alloc_bytes).sum();
    let recoverable: u64 = ctx
        .candidates
        .iter()
        .filter(|c| {
            matches!(
                c.grade,
                Grade::PureCache | Grade::RebuildableFromSource | Grade::Redownloadable
            )
        })
        .map(|c| c.alloc_bytes)
        .sum();
    ToolOutput::ScanSummary {
        volumes: ctx.vols.len() as u32,
        files: ctx.files_scanned,
        bytes,
        candidates: ctx.candidates.len() as u32,
        recoverable_bytes: recoverable,
    }
}

#[derive(Deserialize)]
struct CandidatesArgs {
    #[serde(default)]
    filter: Option<CandidatesFilter>,
}

#[derive(Deserialize)]
struct CandidatesFilter {
    #[serde(default)]
    grade: Option<String>,
    #[serde(default)]
    min_bytes: Option<u64>,
    #[serde(default)]
    limit: Option<u32>,
}

fn tool_candidates(ctx: &SessionCtx, args: &serde_json::Value) -> ToolOutput {
    let args: CandidatesArgs = match serde_json::from_value(args.clone()) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutput::Error {
                message: format!("invalid arguments: {e}"),
            };
        }
    };
    let grade = match args.filter.as_ref().and_then(|f| f.grade.as_deref()) {
        Some(g) => {
            match serde_json::from_value::<Grade>(serde_json::Value::String(g.to_string())) {
                Ok(g) => Some(g),
                Err(_) => {
                    return ToolOutput::Error {
                        message: format!("invalid grade: {g}"),
                    };
                }
            }
        }
        None => None,
    };
    let mut items: Vec<CandDto> = ctx
        .candidates
        .iter()
        .filter(|c| grade.is_none_or(|g| c.grade == g))
        .filter(|c| {
            args.filter
                .as_ref()
                .and_then(|f| f.min_bytes)
                .is_none_or(|m| c.alloc_bytes >= m)
        })
        .map(cand_dto)
        .collect();
    if let Some(limit) = args.filter.as_ref().and_then(|f| f.limit) {
        items.truncate(limit as usize);
    }
    ToolOutput::Candidates { items }
}

fn cand_dto(c: &Candidate) -> CandDto {
    let age = SystemTime::now()
        .duration_since(c.mtime)
        .unwrap_or_default()
        .as_secs();
    let bucket = if age < 3_600 {
        MtimeBucket::Hour
    } else if age < 86_400 {
        MtimeBucket::Day
    } else if age < 604_800 {
        MtimeBucket::Week
    } else {
        MtimeBucket::Old
    };
    let evidence = c
        .evidence
        .iter()
        .map(|e| {
            let kind = serde_json::to_value(&e.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "signal".to_string());
            if e.detail.trim().is_empty() {
                kind
            } else {
                format!("{kind}: {}", e.detail)
            }
        })
        .collect();
    CandDto {
        id: c.id.clone(),
        bytes: c.alloc_bytes,
        grade: c.grade,
        evidence,
        mtime_bucket: bucket,
        dir_shape: "build-tree".to_string(),
    }
}

#[derive(Deserialize)]
struct BuildPlanArgs {
    #[serde(default)]
    target_bytes: Option<u64>,
    #[serde(default)]
    min_bytes: Option<u64>,
    #[serde(default)]
    allow_grades: Option<Vec<String>>,
}

fn tool_build_plan(ctx: &mut SessionCtx, args: &serde_json::Value) -> ToolOutput {
    let args: BuildPlanArgs = match serde_json::from_value(args.clone()) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutput::Error {
                message: format!("invalid arguments: {e}"),
            };
        }
    };
    let allow = match args.allow_grades {
        Some(gs) => {
            let mut out = Vec::with_capacity(gs.len());
            for g in gs {
                match serde_json::from_value::<Grade>(serde_json::Value::String(g.clone())) {
                    Ok(g) => out.push(g),
                    Err(_) => {
                        return ToolOutput::Error {
                            message: format!("invalid grade: {g}"),
                        };
                    }
                }
            }
            Some(out)
        }
        None => None,
    };
    let mut pc = PolicyConstraintsShape::default();
    if let Some(t) = args.target_bytes {
        pc.target_bytes = Some(t);
    }
    if let Some(m) = args.min_bytes {
        pc.min_bytes = m;
    }
    if let Some(g) = allow {
        pc.allow_grades = g;
    }
    let plan = plan::build(&ctx.candidates, &pc, &ctx.vols);
    ctx.last_plan_id = Some(plan.plan_id.clone());
    ctx.last_plan_bytes = Some(plan.totals.quarantine_bytes);
    let gib = |b: u64| b as f64 / 1024.0f64.powi(3);
    let summary = format!(
        "quarantine {:.1} GiB, {} entries; refused {}",
        gib(plan.totals.quarantine_bytes),
        plan.totals.quarantine_entries,
        plan.totals.refused
    );
    ToolOutput::PlanBuilt {
        plan_id: plan.plan_id,
        quarantine_entries: plan.totals.quarantine_entries,
        quarantine_bytes: plan.totals.quarantine_bytes,
        refused: plan.totals.refused,
        summary,
    }
}

#[derive(Deserialize)]
struct ApplyPlanArgs {
    plan_id: String,
}

fn tool_apply_plan(ctx: &mut SessionCtx, args: &serde_json::Value) -> ToolOutput {
    let args: ApplyPlanArgs = match serde_json::from_value(args.clone()) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutput::Error {
                message: format!("invalid arguments: {e}"),
            };
        }
    };
    let Some(last) = &ctx.last_plan_id else {
        return ToolOutput::Error {
            message: "no plan has been built in this run".to_string(),
        };
    };
    if &args.plan_id != last {
        return ToolOutput::Error {
            message: format!("unknown plan_id: {}", args.plan_id),
        };
    }
    ToolOutput::Proposed {
        plan_id: args.plan_id,
        bytes: ctx.last_plan_bytes.unwrap_or(0),
        awaiting: "human-confirm".to_string(),
    }
}

#[derive(Deserialize)]
struct RestoreOpArgs {
    op_id: String,
}

fn tool_restore_op(args: &serde_json::Value) -> ToolOutput {
    let args: RestoreOpArgs = match serde_json::from_value(args.clone()) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutput::Error {
                message: format!("invalid arguments: {e}"),
            };
        }
    };
    ToolOutput::RestoreProposed {
        op_id: args.op_id,
        awaiting: "human-confirm".to_string(),
    }
}

#[derive(Deserialize)]
struct ReceiptsArgs {
    #[serde(default)]
    limit: Option<u32>,
}

fn tool_ledger_receipts(
    run: &mut AgentRun,
    ctx: &SessionCtx,
    args: &serde_json::Value,
) -> ToolOutput {
    let args: ReceiptsArgs = match serde_json::from_value(args.clone()) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutput::Error {
                message: format!("invalid arguments: {e}"),
            };
        }
    };
    // M4: the ledger Store has no read API yet (write-only by design), so receipts are
    // session-scoped: seeded once from the caller's candidate set. Full ledger-history
    // receipts land with the ledger read API (M5+).
    if run.receipts.is_empty() && !ctx.candidates.is_empty() {
        run.receipts = ctx
            .candidates
            .iter()
            .map(|c| ReceiptDto {
                id: c.id.clone(),
                verdict: "candidate".to_string(),
                bytes: c.alloc_bytes,
                grade: c.grade,
            })
            .collect();
    }
    let limit = args.limit.unwrap_or(20) as usize;
    ToolOutput::Receipts {
        items: run.receipts.iter().take(limit).cloned().collect(),
    }
}

#[derive(Deserialize)]
struct VerdictArgs {
    verdict: String,
    #[serde(default)]
    reason: Option<String>,
}

fn tool_record_verdict(args: &serde_json::Value) -> ToolOutput {
    let args: VerdictArgs = match serde_json::from_value(args.clone()) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutput::Error {
                message: format!("invalid arguments: {e}"),
            };
        }
    };
    ToolOutput::VerdictRecorded {
        verdict: args.verdict,
        reason: args.reason,
    }
}

// --- THE chokepoint ------------------------------------------------------------------

/// THE chokepoint: the ONLY place tool results become LLM-visible JSON. Every string
/// leaf in the output tree is scrubbed by `scrub_text`; nothing escapes unscrubbed.
pub fn serialize_tool_result(o: &ToolOutput) -> String {
    let v = serde_json::to_value(o).expect("ToolOutput is serializable");
    serde_json::to_string(&scrub_value(v)).expect("scrubbed value serializes")
}

fn scrub_value(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => serde_json::Value::String(scrub_text(&s)),
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.into_iter().map(scrub_value).collect())
        }
        serde_json::Value::Object(m) => {
            serde_json::Value::Object(m.into_iter().map(|(k, x)| (k, scrub_value(x))).collect())
        }
        other => other,
    }
}

/// Redact path/name hints from one free-text run: any whitespace-delimited token that
/// looks path-like (`/`, `~`, `.`/`..`) or carries a name hint (`.app`/`.dmg`/`.trash`,
/// `node_modules`, `deriveddata`, `_cacache`, `pnpm`, `.git`) becomes `[redacted]`.
pub fn scrub_text(s: &str) -> String {
    s.split_whitespace()
        .map(|tok| {
            if is_leaky_token(tok) {
                "[redacted]"
            } else {
                tok
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_leaky_token(tok: &str) -> bool {
    if tok.contains('/') || tok.contains('~') || tok == "." || tok == ".." {
        return true;
    }
    let l = tok.to_ascii_lowercase();
    l.contains(".app")
        || l.contains(".dmg")
        || l.contains(".trash")
        || l.contains("node_modules")
        || l.contains("deriveddata")
        || l.contains("_cacache")
        || l.contains("pnpm")
        || l.contains(".git")
}

/// The canonical leak set — the SPEC §3.7 leak discipline expressed as contains-checks
/// (regex crate is not a dependency; these cover the same ground, strictly stronger:
/// `~`, `/Users/` `/Volumes/` `/home/`, `.app`/`.dmg`/`.trash` names, and the
/// node_modules/DerivedData/_cacache/pnpm name hints). Returns the first match.
pub fn leaks_path_identifiers(s: &str) -> Option<&'static str> {
    if s.contains('~') {
        return Some("~");
    }
    if s.contains("/Users/") || s.contains("/Volumes/") || s.contains("/home/") {
        return Some("absolute path");
    }
    let l = s.to_ascii_lowercase();
    if l.contains(".app") || l.contains(".dmg") || l.contains(".trash") {
        return Some("app/dmg/trash name");
    }
    if l.contains("node_modules")
        || l.contains("deriveddata")
        || l.contains("_cacache")
        || l.contains("pnpm")
    {
        return Some("name hint");
    }
    None
}

/// FNV-1a (same recipe as `crate::path_hash`) over name + canonical args JSON — the
/// per-call retry hash (tool_retry_limit=3 per name+args, note 07 §wire).
fn retry_key(name: &str, args: &serde_json::Value) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    if let Ok(bytes) = serde_json::to_vec(args) {
        for b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

// --- leak tests: serde round-trip every variant through the chokepoint ---------------

#[cfg(test)]
mod leak_tests {
    use super::*;

    fn assert_path_blind(o: &ToolOutput) {
        // serde round-trip (structural): ToolOutput survives plain serialize → deserialize.
        let plain = serde_json::to_string(o).expect("ToolOutput serializes");
        let back: ToolOutput = serde_json::from_str(&plain).expect("round-trip");
        assert_eq!(&back, o, "round-trip changed the output");
        // The chokepoint form is what the model sees: scrubbed and path-free.
        let json = serialize_tool_result(o);
        assert!(
            leaks_path_identifiers(&json).is_none(),
            "leak in chokepoint output: {json}"
        );
    }

    /// Every ToolOutput variant, each seeded with worst-case path/name-laden strings.
    #[test]
    fn every_variant_is_path_blind() {
        assert_path_blind(&ToolOutput::ScanSummary {
            volumes: 2,
            files: 1234,
            bytes: 1 << 30,
            candidates: 3,
            recoverable_bytes: 1 << 29,
        });
        assert_path_blind(&ToolOutput::Candidates {
            items: vec![CandDto {
                id: "C-001".into(),
                bytes: 1 << 30,
                grade: Grade::PureCache,
                evidence: vec![
                    "/Users/aaikyn1/Code/My App.app node_modules ~/DerivedData".into(),
                    "pnpm store _cacache hint".into(),
                    "lockfile present".into(),
                ],
                mtime_bucket: MtimeBucket::Old,
                dir_shape: "build-tree".into(),
            }],
        });
        assert_path_blind(&ToolOutput::PlanBuilt {
            plan_id: "pl-1".into(),
            quarantine_entries: 2,
            quarantine_bytes: 1 << 30,
            refused: 1,
            summary: "/Volumes/backup: DerivedData/ project is ~/.Trash".into(),
        });
        assert_path_blind(&ToolOutput::Proposed {
            plan_id: "pl-1".into(),
            bytes: 1 << 30,
            awaiting: "human-confirm".into(),
        });
        assert_path_blind(&ToolOutput::RestoreProposed {
            op_id: "op-1".into(),
            awaiting: "human-confirm".into(),
        });
        assert_path_blind(&ToolOutput::Receipts {
            items: vec![ReceiptDto {
                id: "C-007".into(),
                verdict: "/home/me/x node_modules".into(),
                bytes: 1 << 20,
                grade: Grade::HandsOff,
            }],
        });
        assert_path_blind(&ToolOutput::VerdictRecorded {
            verdict: "proceed".into(),
            reason: Some("/Users/aaikyn1/.Trash/App.dmg".into()),
        });
        assert_path_blind(&ToolOutput::Error {
            message: "cannot touch /Users/aaikyn1/node_modules/_cacache ~/App.app".into(),
        });
    }

    /// A path smuggled into candidate evidence detail must not survive the chokepoint.
    #[test]
    fn crafted_candidate_smuggle_is_scrubbed() {
        let path = "/Users/aaikyn1/Library/Developer/Xcode/DerivedData";
        let out = ToolOutput::Candidates {
            items: vec![CandDto {
                id: "C-001".into(),
                bytes: 1 << 30,
                grade: Grade::RebuildableFromSource,
                evidence: vec![format!("orphan_tree: {path}")],
                mtime_bucket: MtimeBucket::Week,
                dir_shape: "build-tree".into(),
            }],
        };
        let json = serialize_tool_result(&out);
        assert!(!json.contains(path), "smuggled path survived: {json}");
        assert!(!json.contains("DerivedData"), "name hint survived: {json}");
        assert!(json.contains("orphan_tree"), "evidence text lost: {json}");
        assert!(leaks_path_identifiers(&json).is_none());
    }

    /// The scrub itself: path tokens redacted, plain tokens preserved.
    #[test]
    fn scrub_redacts_only_leaky_tokens() {
        assert_eq!(scrub_text("ok"), "ok");
        assert_eq!(scrub_text("/Users/aaikyn1 ~/x"), "[redacted] [redacted]");
        assert_eq!(scrub_text("My App.app"), "My [redacted]");
        assert_eq!(scrub_text("NODE_MODULES cache"), "[redacted] cache");
        assert_eq!(
            scrub_text("lockfile present: package-lock.json"),
            "lockfile present: package-lock.json"
        );
    }

    /// cost_of: hit/miss/completion split per 1M (§3.7 formula; note 07 §cost).
    #[test]
    fn cost_formula() {
        let p = ModelPrice {
            model: "m".into(),
            prompt_miss_per_1m: 0.27,
            prompt_hit_per_1m: 0.07,
            completion_per_1m: 1.10,
        };
        // All cached prompt (1M) + 0.5M completion: 0.07 + 0.55.
        let u = Usage {
            prompt_tokens: 1_000_000,
            completion_tokens: 500_000,
            cached_tokens: 1_000_000,
        };
        assert!((cost_of(&u, &p) - 0.62).abs() < 1e-9);
        // Uncached prompt (1M) + completion (0.5M): 0.27 + 0.55.
        let u2 = Usage {
            prompt_tokens: 1_000_000,
            completion_tokens: 500_000,
            cached_tokens: 0,
        };
        assert!((cost_of(&u2, &p) - 0.82).abs() < 1e-9);
    }
}

/// HTTP `ChatClient` (feature `llm`) — the async-openai/reqwest implementation of the
/// injected model client; see `agent::http` for provider quirks (DeepSeek/Ollama/
/// OpenRouter) and the runtime confinement note (SPEC §2.1).
#[cfg(feature = "llm")]
pub mod http;
