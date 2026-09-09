//! `cost/` — R6 token & cost accounting (SPEC Part B §3.7 + research note 07 §cost).
//!
//! Reads `Kind::Budget` records out of the ledger (each agent-loop call persists its
//! provider-reported usage as a `LedgerRecord::Budget`; `agent/` owns the writing) and
//! rolls them up into the D-20 开销明细表: one row per (date, model), columns
//! 日期 / 模型 / 调用 / prompt+completion tokens / USD / CNY. `total_usd` is the running
//! cost that feeds the R6 budget ⇒ interrupt check.
//!
//! Provider-reported usage is ground truth — never a local tokenizer for billing
//! (§3.7 / note 07 §cost). `estimate_unverified` exists only for the lost-usage-chunk
//! fallback and is explicitly UNVERIFIED: it never feeds Budget records.
//!
//! FX: `USD_CNY` is a STATIC rate — D-20 scope is LLM API spend only, not a live FX feed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::ledger::{self, LedgerRecord};

/// Static USD→CNY rate for the D-20 开销明细表. A documented constant, not a live FX
/// feed — D-20 scope is LLM API spend only.
pub const USD_CNY: f64 = 7.2;

/// One (date, model) group — the D-20 开销明细表 row unit.
#[derive(Debug, Clone, PartialEq)]
pub struct CostRow {
    pub date: chrono::NaiveDate,
    pub model: String,
    pub calls: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cost_usd: f64,
    pub cost_cny: f64,
}

/// Ledger file path the cost rollup reads by default: `<home>/ledger.jsonl`, where home
/// is `DISK_JANITOR_HOME` or `~/.disk-janitor` (see `execute::home_ledger_dir`).
pub fn default_ledger_path() -> PathBuf {
    crate::execute::home_ledger_dir().join("ledger.jsonl")
}

/// Reads budget recs from the ledger Store (SPEC §3.7: per-call usage persisted to the
/// ledger; running cost = Σ). The path is injectable for tests; `default()` targets the
/// home ledger.
#[derive(Debug, Clone)]
pub struct CostLedger {
    path: PathBuf,
}

impl CostLedger {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn with_default_path() -> Self {
        Self::new(default_ledger_path())
    }

    pub fn rows(&self) -> Result<Vec<CostRow>> {
        rows(&self.path)
    }

    pub fn total_usd(&self) -> Result<f64> {
        total_usd(&self.path)
    }
}

/// Roll up `Kind::Budget` records in the ledger file `path` into one row per
/// (date, model). Every other record kind is ignored — only budget recs count toward
/// LLM API spend. Rows come back ordered by date, then model.
pub fn rows(path: &Path) -> Result<Vec<CostRow>> {
    let (records, _report) = ledger::replay(path)?;
    let mut groups: BTreeMap<(chrono::NaiveDate, String), CostRow> = BTreeMap::new();
    for rec in records {
        let LedgerRecord::Budget {
            model,
            calls,
            prompt_tokens,
            completion_tokens,
            cost_usd,
        } = rec.rec
        else {
            continue; // not a budget rec — no LLM spend to account
        };
        let date = rec.ts.date_naive();
        let g = groups.entry((date, model.clone())).or_insert(CostRow {
            date,
            model: model.clone(),
            calls: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: 0.0,
            cost_cny: 0.0,
        });
        g.calls += calls;
        g.prompt_tokens += prompt_tokens;
        g.completion_tokens += completion_tokens;
        g.cost_usd += cost_usd;
    }
    for g in groups.values_mut() {
        g.cost_cny = g.cost_usd * USD_CNY;
    }
    Ok(groups.into_values().collect())
}

/// Running LLM API spend in USD (Σ cost over all Budget recs) — the input to the R6
/// budget check: interrupt the agent loop when `≥ config.budget.cost_usd`.
pub fn total_usd(path: &Path) -> Result<f64> {
    Ok(rows(path)?.iter().map(|r| r.cost_usd).sum())
}

/// Provider-reported per-call usage (SPEC §3.7 / note 07 §cost). Reasoning tokens are
/// already inside `completion_tokens` — never double-counted. This is the sans-IO shape
/// the agent loop feeds; the real `async-openai` `CompletionUsage` maps onto it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageLike {
    pub prompt_tokens: u64,
    /// Prompt cache-hit portion of `prompt_tokens` (DeepSeek `prompt_cache_hit_tokens`,
    /// OpenAI/OpenRouter `prompt_tokens_details.cached_tokens`).
    pub cached_tokens: u64,
    pub completion_tokens: u64,
}

/// Split prompt tokens into (cache-miss, cache-hit, completion) for the §3.7 cost
/// formula `(prompt − cached)·p_miss + cached·p_hit + completion·p_completion`.
/// The hit portion is clamped to `prompt_tokens` so a misreporting provider can never
/// produce negative miss tokens (cost stays honest, no underflow).
pub fn effective_usage(u: &UsageLike) -> (u64, u64, u64) {
    let cached = u.cached_tokens.min(u.prompt_tokens);
    (u.prompt_tokens - cached, cached, u.completion_tokens)
}

/// tiktoken-style token estimate — chars/4 heuristic. **UNVERIFIED**: approximate for
/// every tokenizer except OpenAI's (DeepSeek/Ollama use their own BPE, note 07 §context),
/// so it is display/preflight only and **MUST NOT feed `Kind::Budget` ledger records** —
/// provider-reported usage is the only billing source (§3.7 lost-usage-chunk fallback:
/// estimate for display, never billed).
pub fn estimate_unverified(prompt: &str, completion: &str) -> u64 {
    (prompt.chars().count() + completion.chars().count()) as u64 / 4
}

/// D-20 开销明细表 as an aligned ASCII table: header 日期 date | 模型 model | 调用 calls |
/// tokens 提示+补全 | USD | CNY, one row per (date, model), TOTAL row last. The tokens
/// column renders as `prompt/completion`. Format is stable — M5 exports it verbatim.
pub fn render_table(rows: &[CostRow]) -> String {
    let hdr = [
        "日期 date".to_string(),
        "模型 model".to_string(),
        "调用 calls".to_string(),
        "tokens 提示+补全".to_string(),
        "USD".to_string(),
        "CNY".to_string(),
    ];

    let fmt_tokens = |p: u64, c: u64| format!("{p}/{c}");
    let fmt_money = |v: f64| format!("{v:.6}");

    let mut cells: Vec<[String; 6]> = Vec::with_capacity(rows.len());
    for r in rows {
        cells.push([
            r.date.to_string(),
            r.model.clone(),
            r.calls.to_string(),
            fmt_tokens(r.prompt_tokens, r.completion_tokens),
            fmt_money(r.cost_usd),
            fmt_money(r.cost_cny),
        ]);
    }

    let total_calls: u32 = rows.iter().map(|r| r.calls).sum();
    let total_prompt: u64 = rows.iter().map(|r| r.prompt_tokens).sum();
    let total_completion: u64 = rows.iter().map(|r| r.completion_tokens).sum();
    let total_usd: f64 = rows.iter().map(|r| r.cost_usd).sum();
    let total_cny: f64 = rows.iter().map(|r| r.cost_cny).sum();
    let total = [
        "TOTAL".to_string(),
        String::new(),
        total_calls.to_string(),
        fmt_tokens(total_prompt, total_completion),
        fmt_money(total_usd),
        fmt_money(total_cny),
    ];

    let mut widths = [0usize; 6];
    for i in 0..6 {
        widths[i] = hdr[i].chars().count();
        for c in cells.iter().chain(std::iter::once(&total)) {
            widths[i] = widths[i].max(c[i].chars().count());
        }
    }

    let mut out = String::new();
    push_row(&mut out, &widths, &hdr);
    for c in &cells {
        push_row(&mut out, &widths, c);
    }
    let line_w = out.lines().next().map_or(0, |l| l.chars().count());
    out.push_str(&"-".repeat(line_w));
    out.push('\n');
    push_row(&mut out, &widths, &total);
    out
}

fn push_row(out: &mut String, widths: &[usize; 6], row: &[String; 6]) {
    for (i, cell) in row.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        let pad = " ".repeat(widths[i].saturating_sub(cell.chars().count()));
        if i <= 1 {
            out.push_str(cell); // 日期 / 模型: left-aligned
            out.push_str(&pad);
        } else {
            out.push_str(&pad); // numeric columns: right-aligned
            out.push_str(cell);
        }
    }
    out.push('\n');
}
