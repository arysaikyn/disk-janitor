# 开销明细表 — LLM API Expense Sheet (D-20, M5)

**Scope: LLM API spend only.** Every row comes from provider-reported usage
persisted as `Kind::Budget` ledger records (`src/ledger.rs`, SPEC Part B §3.7 /
research note 07 §cost). Local tokenizer estimates (`cost::estimate_unverified`)
are never billed — provider-reported usage is the only billing source. One row
per (date, model): 日期 / 模型 / 调用 / prompt+completion tokens / USD / CNY,
TOTAL row last. CNY = USD × `cost::USD_CNY` (7.2, a documented static rate — not
a live FX feed; D-20 scope is LLM API spend only).

## 生成 Generation (P7 演示可复现)

Every table below is generated from a ledger — never hand-typed:

```bash
# Real state — current home ledger (DISK_JANITOR_HOME or ~/.disk-janitor):
DISK_JANITOR_HOME=<home> ./scripts/gen-expense.sh > docs/expense-sheet.md

# 演示数据 demo fixture — tempdir ledger seeded with demo Budget records:
./scripts/gen-expense.sh --demo
```

Pipeline: `scripts/gen-expense.sh` → `cargo run --example expense-report`
(`examples/expense-report.rs`) → `disk_janitor::cost::{CostLedger, render_table}`
(`src/cost.rs`). Ledger envelope: frozen v1 (`src/ledger.rs`), per-record
`crc32 = crc32fast(serde_json::to_vec(rec) ++ seq.to_le_bytes())` — replay
validates every line, so a corrupt or hand-edited record is skipped, never shown.

## 当前实况 Current state (2026-09-10)

No live LLM API calls have been made yet — the R6 agent loop is not wired to a
provider — so the real home ledger (`~/.disk-janitor/ledger.jsonl`) contains
**no `Budget` records**. Fresh run of the generator against a tempdir ledger
(empty `ledger.jsonl`, exactly what `ledger::Store::open` creates):

```text
日期 date  模型 model  调用 calls  tokens 提示+补全        USD        CNY
---------------------------------------------------------------
TOTAL                     0           0/0  -0.000000  -0.000000
```

(The `-0.000000` USD/CNY cells are Rust std's negative-zero rendering of an empty
`f64` sum on the pinned 1.97.1 toolchain — cosmetic; any real record sums to ≥ 0.)

## 演示数据 Demo — NOT real API spend

「演示数据 demo — generated from a tempdir fixture ledger, not real API spend」

Seeded by `./scripts/gen-expense.sh --demo`: 3 demo `Budget` records (frozen v1
envelopes, CRC-validated on replay) written to a throwaway tempdir ledger, then
rolled up by the same generator:

```text
日期 date     模型 model       调用 calls  tokens 提示+补全       USD       CNY
2026-09-08  deepseek-chat         3      1500/450  0.016000  0.115200
2026-09-09  gpt-4o-mini           3      1200/400  0.003456  0.024883
---------------------------------------------------------------------
TOTAL                             6      2700/850  0.019456  0.140083
```
