# 还我磁盘 Giveback

**A disk-cleanup app that tells you what's safe to delete, shows the evidence, and
lets you undo everything.** Native macOS desktop app (Tauri + Svelte + Rust). Course
project for 程序设计训练（Rust 语言）(30240522-3), Tsinghua CS, 2025–2026 summer term.

**Status: M1–M4 implemented and verified (126 tests green across 13 suites; CLI + GUI
flows proven end-to-end) — M5 submission deliverables shipped 2026-09-14, tracked in
[`docs/deliverables-checklist.md`](docs/deliverables-checklist.md).** Start reading at
[`docs/SPEC.md`](docs/SPEC.md) (Part A = overview; History keeps the real timeline).

> **公开镜像说明（2026-10-28）：** `docs/ai-logs/`（AI 对话历史，含个人上下文）未包含在本仓库；代码、SPEC、设计文档与交付清单均与课程提交版本一致。

## Why this exists

`du` tells you *where* the bytes are; it never tells you whether they are *safe to
remove*. Every existing tool stops at byte locations or black-box verdicts. Disk Janitor:

1. **Judges with evidence, not names** — every candidate carries an inspectable
   evidence chain (lockfile present? sibling manifest gone? written 3 min ago →
   refuse) and a rebuild-cost grade (PureCache → HandsOff).
2. **Makes deletion reversible by default** — `apply` moves to an on-volume
   quarantine (atomic, zero bytes freed); `purge` requires typing the exact byte
   count; 72-hour `restore` is one command. 删除是操作，恢复是权利。
3. **Keeps memory** — an append-only JSONL ledger (台账) remembers every verdict and
   operation across sessions, so the tool never re-judges from zero.
4. **Uses AI without giving it authority** — the agent understands "腾出 25GB，别动
   我这两周在跑的项目" and proposes plans, but never deletes; it never even sees a
   file path (privacy chokepoint + CI leak test).

## What works today (M1–M4)

- **`janitor scan --json` CLI** — dua-core 4.0.0 macOS walker, deterministic
  candidate ids, evidence engine, rebuild-cost grades [9f4cb7c].
- **plan / apply / purge / restore** — `renamex_np` on-volume quarantine (atomic,
  no-clobber), crc32-framed JSONL WAL + `flock`, append-only ledger; **kill -9 at
  any point recovers cleanly** [9fd33f1, 17647bb].
- **GUI (Tauri 2.11.5 + Svelte 5)** — dashboard / scan / review / plan / ledger /
  purge-gate / settings, zh-en i18n; full flow verified in the window: scan 51 →
  plan 44 → apply 17.7 GB → typed-gate purge → restore [97d704f].
- **NL policy (MTA)** — 「腾出 25 GB」 → plan (3 quarantine / 4 refuse) → apply →
  recover → restore end-to-end; 「别动我这两周在跑的项目」 protects project
  activity via `.git/HEAD`, exempts build outputs [77f0b97].
- **AI without authority** — sans-IO agent proposes plans only; path-blind
  chokepoint, budget interrupts, leak tests [a671575]; config.toml + env overrides
  + redact() [3ed9545].
- **Typed-byte purge gate** — purge demands the exact byte count typed.

## Quickstart

```bash
# 1. Build everything (Rust core + CLI)
cargo build

# 2. CLI flow — scan your home dir, review what's reclaimable
./target/debug/janitor scan --json > /tmp/scan.json        # candidates as JSON
./target/debug/janitor plan /tmp/scan.json --policy "腾出 25 GB" --min-bytes 104857600
./target/debug/janitor apply plan-*.json                   # zero bytes freed, reversible
./target/debug/janitor restore <op-id>                     # one command undo (within 72 h)
./target/debug/janitor purge plan-*.json --bytes <EXACT>   # irreversible — type the byte count

# 3. GUI (Tauri + Svelte) — needs Bun and a vite dev server
cd app && bun install && bun run dev        # frontend on :1420
cd ../src-tauri && cargo run                # app window; add --demo-data for synthetic data

# 4. AI agent (needs an API key; D-22: demo runs without one)
DISK_JANITOR_API_KEY=sk-... cargo run --features llm --bin janitor -- agent /tmp/scan.json --prompt "腾出 25 GB"
```

Crash safety: `kill -9` the app anywhere between scan and purge — next launch
`janitor recover` reconciles the WAL (apply auto-resumes; purge runs to completion).

## 配置 Endpoint / Key

`config/`（R3）读取 **一个** TOML 文件，再用环境变量逐字段覆盖它。默认值齐全，
所以配置文件**可以不写**——四个字段都有 `#[serde(default)]`，部分文件会合并到默认值
之上，而不是因缺字段报错（E1）。

配置文件位置 = `<config_dir>/disk-janitor/config.toml`（`dirs::config_dir()`）：

- macOS — `~/Library/Application Support/disk-janitor/config.toml`
- Linux — `~/.config/disk-janitor/config.toml`
- Windows — `%APPDATA%\disk-janitor\config.toml`

```toml
# <config_dir>/disk-janitor/config.toml —— 只写你要改的字段，其余走默认值
endpoint = "https://api.deepseek.com/v1"   # 默认值；任何 OpenAI 兼容端点都可
api_key  = "sk-..."                        # 绝不写回、绝不记日志
model    = "deepseek-chat"                 # 默认值
ctx_len  = 65536                           # R3
thinking = false                           # R3

[[prices]]                                 # D-20：按模型登记每 1M token 单价
model = "deepseek-chat"
prompt_miss_per_1m = 0.27
prompt_hit_per_1m  = 0.07
completion_per_1m  = 1.10

[budget]                                   # R6：超预算即中断
cost_usd  = 1.0
max_turns = 12
tokens    = 200000
```

环境变量覆盖（优先级高于上述文件，四个都支持）：

| 变量 | 覆盖字段 |
|---|---|
| `DISK_JANITOR_ENDPOINT` | `endpoint` |
| `DISK_JANITOR_API_KEY` | `api_key` |
| `DISK_JANITOR_MODEL` | `model` |
| `DISK_JANITOR_BUDGET_USD` | `budget.cost_usd` |

```bash
DISK_JANITOR_API_KEY=sk-... DISK_JANITOR_MODEL=deepseek-chat \
  cargo run --features llm --bin janitor -- agent /tmp/scan.json --prompt "腾出 25 GB"
```

三条与密钥有关的不变量：`api_key` 标注 `#[serde(skip_serializing)]`，**永不写回磁盘**；
`Debug` 是手写实现，密钥只渲染成 `***` 或 `none`（E3）；`get_model_config` 这条 GUI
命令也从不序列化密钥，webview 只能拿到 `has_key`（P4）。GUI 的 设置 → 模型 面板编辑的是
同一个文件（`save_model_config` 原子替换之）。

endpoint 的尾部 `/` 会被剥掉（E2）——async-openai 会无条件拼接 `/chat/completions`，
`…/v1/` 会变成 `//chat/completions` 而 404。

**无密钥也能跑（D-22）**：`--demo-data` 让全部 Tauri 命令走合成数据，演示不需要 Key；
CLI 的 `scan` / `plan` / `apply` / `restore` / `purge` 整条链路同样完全本地、不联网，
只有 `agent` 子命令（`--features llm`）需要 Endpoint 与 Key。

## 演示用例

**A. 真实数据（本机家目录）** —— 一条完整、可逆的清理链路：

```bash
./target/debug/janitor scan --json > /tmp/scan.json          # 扫描家目录，产出候选
./target/debug/janitor plan /tmp/scan.json --policy "腾出 25 GB，别动我这两周在跑的项目"
./target/debug/janitor apply plan-*.json                      # 移入隔离区，释放 0 字节
./target/debug/janitor restore <op-id>                        # 72 h 内一条命令撤销
./target/debug/janitor purge plan-*.json --bytes <EXACT>      # 不可逆，必须键入精确字节数
```

`--policy` 走本地规则编译器（MTA 优先匹配），这条句子在本地即编译完成、**不触发 LLM**：
它保护 `.git/HEAD` mtime 落在两周内的项目，同时豁免机器派生的构建产物。

**B. 合成数据（推荐用于演示，绝不碰真实文件）** —— `fixture-gen` 造出已知形态的垃圾树：

```bash
cargo run --bin fixture-gen -- /tmp/demo-tree                 # 10 个 fixture 集，会打印汇总
./target/debug/janitor scan --json --min-bytes 32768 /tmp/demo-tree > /tmp/demo.json
./target/debug/janitor plan /tmp/demo.json --policy "腾出 25 GB" --min-bytes 32768
```

上面两条都带 `--min-bytes 32768`：`scan` 的默认阈值是 50 MiB（面向真实家目录），而
合成树只有 32 KiB – 2 MB —— 不降阈值会扫出 0 个候选。跑通后 `plan` 会打印
`quarantine 3 entries; refuse 4`：`_cacache`、`demo-app/node_modules`、`DerivedData`
进隔离计划；`.venv`、`orphan-node/node_modules`、`pnpm-linked/node_modules` 以
`orphan_tree` 被拒，`hot-project/node_modules` 以 `mtime_hot` 被拒——后者正是 MTA
「别动我在跑的项目」判据在合成数据上的可见证据。

fixture 集：`demo-app`（node_modules + 锁文件）、`orphan-node`（祖先无 manifest ⇒ orphan）、
`venv`、`derived-data`、`cacache`（CAS 纯缓存）、`pnpm-store`（硬链接）、`sparse`
（稀疏文件，按 allocated 字节计，不按逻辑大小）、`symlink-chain`（链 + 环）、
`trash-janitor`、`hot-project`（活动中的项目，应被 MTA 豁免）。`--set <name>` 只造一个，
`--scale N` 放大文件数。测试与演示只跑 `tests/fixtures/` 或临时目录下的合成树。

**C. GUI** —— `cd app && bun install && bun run dev`，另一个终端
`cd src-tauri && cargo run -- --demo-data`（合成数据，无需 Key）。窗口内可完整走通
扫描 → 计划复核 → apply → 键入字节数的 purge 闸门 → restore。

## Test / gate commands

```bash
cargo fmt --check && cargo clippy -D warnings && cargo test   # 126 tests, 13 suites (fixture trees only)
cargo test --features llm                                    # 136 tests (adds the HTTP client suite)
```

## Milestones

| Milestone | Status | Evidence |
|---|---|---|
| M1 scan + classify core | ✅ | 9f4cb7c (dua-core 3.3.0 pin at the time — bumped to `=4.0.0` 09-10 per D-37; syscall tests, fixture generator) |
| M2 plan/execute + WAL + ledger | ✅ | 9fd33f1, 17647bb (crash injection, restore keep-both, non-UTF8 refusal) |
| M3 GUI shell | ✅ | 97d704f (Tauri 2.11.5, 17 IPC commands, writable-vol dedup) |
| M4 config + agent + cost | ✅ | 3ed9545, a671575, 77f0b97 (NL policy E2E 腾出 25 GB) |
| M5 submission deliverables | ✅ | design-document PDF · Tsinghua Git source · `docs/ai-logs/raw/` (18 sessions, full conversation history) · [`docs/expense-sheet.xlsx`](docs/expense-sheet.xlsx) — [`docs/deliverables-checklist.md`](docs/deliverables-checklist.md) |

## Documentation

| Doc | Contents |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | **single source of truth** — Parts A product · B technical · C UX · D dependencies, status & milestones |
| [`docs/design-decisions.md`](docs/design-decisions.md) | 17 ADRs + the D-01…D-31 decision register (appendix). D-32…D-38 (2026-09-10) are recorded separately in [`docs/decision-sheet-2026-09-10.md`](docs/decision-sheet-2026-09-10.md) |
| [`docs/research/`](docs/research/) | 33 verified research notes + `archive/` (competitor survey), cited as [NN] |
| `docs/ai-logs/` | AI 对话历史原始记录 — **full conversation history** in `raw/` (one Markdown file per session + `index.md`), with per-milestone summaries `M1.md`…`M5.md` (graded deliverable, redacted; generated by `scripts/gen-ai-transcript.py`) |
| [`docs/expense-sheet.md`](docs/expense-sheet.md) · [`expense-sheet.xlsx`](docs/expense-sheet.xlsx) | 开销明细表 — AI development cost by phase (graded deliverable, Excel + readable markdown; generated by `scripts/gen-expense-xlsx.py`) |

## License / provenance

Course submission — pushed to the Tsinghua Git server; LICENSE decision deferred
until after grading (decision register D-07 — appendix of `docs/design-decisions.md`).
AI-assisted development throughout; conversation logs are a graded deliverable
(full session history exported to `docs/ai-logs/raw/`, per-milestone summaries in
`docs/ai-logs/M<1-5>.md`).
