# 开销明细表 — AI development cost by phase (M5 / 作业要求 item 4)

Course deliverable: **「AI 开发开销明细表 Excel（分阶段: 人时 / API次数 / tokens / 花费 / 模型 / 工具）」** —
`~/vault/Courses/程序设计训练(Rust)/Agent-Project.md`, deliverables item 4.

**Machine-readable copy: [`docs/expense-sheet.xlsx`](expense-sheet.xlsx)** — four sheets
(分阶段 / 逐日 / 工具 / 方法). Regenerate both the workbook and the tables below with:

```bash
python3 scripts/gen-expense-xlsx.py
```

**Scope.** This sheet measures the **AI-assisted development process** that produced this
repository — the cost of *building* Disk Janitor. It is NOT the product's own runtime
spend; that ledger is a separate, clearly-labelled table in §3 and is empty today.

## 1. Source and method

| | |
|---|---|
| **Harness** | `omp` (Oh My Pi). Session transcripts: `~/.omp/agent/sessions/-Code-disk_janitor/` |
| **Corpus** | top-level sessions + subagent transcripts (counts in the generated block below) |
| **Extraction** | Every assistant API call carries a `message.usage` record (`input` / `output` / `cacheRead` / `cacheWrite` / `cost.total`), deduplicated by message `id` across all files |
| **Cutoff** | The newest transcript timestamp — so the output is a function of the corpus alone |
| **Phase bucketing** | By message **UTC** day, assigned to the phase whose milestone log (`docs/ai-logs/M1.md`…`M5.md`) covers that day. Day rows sum exactly to the phase rows and the grand total |
| **API次数** | Assistant API calls, i.e. billable round-trips — **not** tool invocations (that is the 工具 column) |
| **人时** | Derived, not observed — see §2 |

`cacheWrite` is **0 on every record** — this gateway bills cache reads only. That is a
property of the gateway, not a measurement gap.

CNY = USD × `cost::USD_CNY` = **7.2** — the same documented static rate the product's own
D-20 cost module uses, not a live FX feed.

⁽¹⁾ **P3's $0.00 is a reporting gap, not free usage.** The `paratera-arys` gateway publishes
no per-token price for `DeepSeek-V4.1-Flash` (the model used for all of P3), so its
`cost.total` field is 0. P3's token counts are real and are included in every token cell.

⁽²⁾ 人时 rounds each row independently to 2 dp, so the displayed column may differ from the
exact total by ±0.01 h. The exact unrounded total is stated in the generated block.

## 2. 分阶段明细表 — cost by phase

<!-- BEGIN GENERATED:expense — edit scripts/gen-expense-xlsx.py, not this block -->

*Snapshot cutoff: **2026-09-13 23:37 +08:00 (= 2026-09-13 15:37 UTC)** — the newest activity in the corpus. Corpus: 17 top-level sessions + 145 subagent transcripts. Both this block and the `.xlsx` are emitted by the same run of `scripts/gen-expense-xlsx.py`.*

| 阶段 phase | 起止 dates | **人时 (h)** | **API次数 calls** | **输入 tokens** | **输出 tokens** | **cache read** | **总 tokens** | **花费 USD** | **花费 CNY** |
|---|---|---|---|---|---|---|---|---|---|
| **P0 立项 · 调研与设计** | 08-30 … 09-08 | 19.70 | 4,402 | 34,716,986 | 2,193,396 | 705,602,560 | 742,512,942 | $36.0503 | ¥259.56 |
| **P1 M1+M2 核心引擎** | 09-09 … 09-09 | 10.37 | 3,139 | 19,372,666 | 2,237,328 | 559,675,840 | 581,285,834 | $15.1942 | ¥109.40 |
| **P2 M3+M4 GUI 与 Agent** | 09-10 … 09-10 | 1.95 | 403 | 3,939,204 | 282,388 | 69,796,864 | 74,018,456 | $2.1356 | ¥15.38 |
| **P3 M5 交付与收尾** | 09-12 … 09-13 | 7.33 | 4,754 | 33,639,287 | 4,228,729 | 360,352,896 | 398,220,912 | $0.0000 | ¥0.00 |
| **合计 TOTAL** | **08-30 … 09-13** | **39.34** | **12,698** | **91,668,143** | **8,941,841** | **1,695,428,160** | **1,796,038,144** | **$53.3800** | **¥384.34** |

### 模型 models (API calls per phase)

| 阶段 | 模型 (API calls) |
|---|---|
| P0 | `glm-5.3-flash` 3,883 · `DeepSeek-V4-Flash-0731` 284 · `glm-5.2` 183 · `GLM-5.3-Flash` 46 · `deepseek-v4-flash-free` 6 |
| P1 | `DeepSeek-V4-Flash-0731` 1,702 · `GLM-5.3-Flash` 1,437 |
| P2 | `DeepSeek-V4-Flash-0731` 282 · `GLM-5.3-Flash` 121 |
| P3 | `DeepSeek-V4.1-Flash` 4,754 |

### 工具 tools (tool invocations per phase)

| 阶段 | 调用总数 | top tools |
|---|---|---|
| P0 | 4,932 | `bash` 1,526 · `read` 1,484 · `grep` 545 · `edit` 348 · `web_search` 334 · `advise` 232 |
| P1 | 3,660 | `bash` 1,186 · `read` 959 · `edit` 640 · `eval` 300 · `grep` 150 · `hub` 135 |
| P2 | 498 | `read` 148 · `bash` 128 · `edit` 77 · `eval` 53 · `grep` 41 · `hub` 30 |
| P3 | 7,864 | `bash` 3,114 · `read` 2,961 · `eval` 616 · `edit` 456 · `grep` 397 · `write` 116 |
| **合计** | **16,954** | `bash` 5,954 · `read` 5,552 · `edit` 1,521 · `grep` 1,133 · `eval` 1,129 · `web_search` 408 |

### 逐日 daily

| 日期 | 阶段 | 人时 h | calls | input | output | cache read | USD |
|---|---|---|---|---|---|---|---|
| 08-30 | P0 | 6.95 | 2,106 | 11,162,106 | 1,130,358 | 212,327,872 | 10.6761 |
| 08-31 | P0 | 4.77 | 751 | 10,277,436 | 352,072 | 179,600,192 | 7.8683 |
| 09-01 | P0 | 5.31 | 1,305 | 11,076,472 | 463,075 | 273,644,224 | 15.9768 |
| 09-06 | P0 | 0.08 | 14 | 47,088 | 5,106 | 394,496 | 0.0238 |
| 09-07 | P0 | 1.41 | 94 | 535,295 | 118,538 | 7,745,152 | 0.3888 |
| 09-08 | P0 | 1.16 | 132 | 1,618,589 | 124,247 | 31,890,624 | 1.1165 |
| 09-09 | P1 | 10.37 | 3,139 | 19,372,666 | 2,237,328 | 559,675,840 | 15.1942 |
| 09-10 | P2 | 1.95 | 403 | 3,939,204 | 282,388 | 69,796,864 | 2.1356 |
| 09-12 | P3 | 3.21 | 2,418 | 11,825,374 | 1,602,010 | 186,828,160 | 0.0000 |
| 09-13 | P3 | 4.12 | 2,336 | 21,813,913 | 2,626,719 | 173,524,736 | 0.0000 |

### 人时 person-hours

Union of all activity windows across the transcripts: timestamps sorted, a gap **> 20 minutes** splits a segment, segments merged globally so overlapping sessions count once, and each segment attributed to the phase of its start day.

| 阶段 | 人时 h | segments |
|---|---|---|
| P0 | 19.70 | 17 |
| P1 | 10.37 | 2 |
| P2 | 1.95 | 1 |
| P3 | 7.33 | 3 |
| **合计** | **39.34** | **23** |

Exact unrounded total: **39.3399 h**. Raw wall-clock span 08-30 07:12 → 09-13 15:37 UTC ≈ 344 h — **not** the number; it includes sleep, class, and other work.

### 会话 session metrics

| Metric | Value | Rule |
|---|---|---|
| Sessions | **17** | top-level transcripts under `~/.omp/agent/sessions/-Code-disk_janitor/` |
| Subagent transcripts | 145 | nested session files (parallel fan-out) |
| API calls | **12,698** | billable assistant round-trips |
| 人时 | **39.34 h** | union of activity windows, 23 segments |
| Elapsed | **15 days** | 08-30 → 09-13 inclusive |

<!-- END GENERATED:expense -->

## 3. 人时 method

人时 = **active working time**, computed as the union of activity windows across every
transcript: timestamps sorted globally, a gap **> 20 minutes** splits a segment, segments
merged so overlapping parallel work counts once, and each segment attributed to the phase
of its start day. Segment counts per phase appear in the generated block above.

The raw wall-clock span is **not** the number — it includes sleep, class, and other work.
This is a **derived** figure, labelled as such everywhere it appears; it is not observed
human hours, and the 20-minute idle threshold is a stated convention.

## 4. Caveats — read before quoting

1. **omp-only.** Claude Code, OpenCode, and Kimi Code sessions are **not** counted. This
   sheet under-counts total AI usage if any of those were used on this project.
2. **Snapshot.** The corpus keeps growing (09-13 is the submission day), so figures are
   frozen at the cutoff stated in the generated block. Re-running the generator moves them.
3. **Phase attribution is by UTC day.** A late-night session on a phase boundary spills
   into the neighbouring UTC day and is attributed there; the day rows sum exactly to the
   phase rows and the grand total, so nothing is lost.
4. **`~/.omp/agent/agent.db` is NOT a usable source** — its `usage_cost_history` holds 9,828
   rows all stamped `provider = opencode-go` with NULL timestamps, and `client_usage` is
   empty. The transcript JSONL is the authority.
5. **P3's $0.00** — see ⁽¹⁾ above.

---

## 5. Runtime ledger — the *product's* own spend (D-20)

> **Not part of 作业要求 item 4.** This section documents what the shipped Disk Janitor
> binary itself records when *it* calls an LLM. It is a different quantity from §2 and is
> kept here because D-20 named this file as its home.

**Scope: LLM API spend only.** Every row comes from provider-reported usage persisted as
`Kind::Budget` ledger records (`src/ledger.rs`, SPEC Part B §3.7 / research note 07 §cost).
Local tokenizer estimates (`cost::estimate_unverified`) are never billed — provider-reported
usage is the only billing source. One row per (date, model): 日期 / 模型 / 调用 /
prompt+completion tokens / USD / CNY, TOTAL row last. CNY = USD × `cost::USD_CNY` (7.2, a
documented static rate — not a live FX feed).

### 生成 Generation (P7 演示可复现)

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

### 当前实况 Current state (2026-09-10)

No live LLM API calls have been made from the product — the R6 agent loop ships with a
keyless fallback (D-22) — so the real home ledger (`~/.disk-janitor/ledger.jsonl`) contains
**no `Budget` records**. Fresh run of the generator against a tempdir ledger
(empty `ledger.jsonl`, exactly what `ledger::Store::open` creates):

```text
日期 date  模型 model  调用 calls  tokens 提示+补全        USD        CNY
---------------------------------------------------------------
TOTAL                     0           0/0  -0.000000  -0.000000
```

(The `-0.000000` USD/CNY cells are Rust std's negative-zero rendering of an empty
`f64` sum on the pinned 1.97.1 toolchain — cosmetic; any real record sums to ≥ 0.)

### 演示数据 Demo — NOT real API spend

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
