# 还我磁盘 Giveback

**A disk-cleanup app that tells you what's safe to delete, shows the evidence, and
lets you undo everything.** Native macOS desktop app (Tauri + Svelte + Rust). Course
project for 程序设计训练（Rust 语言）(30240522-3), Tsinghua CS, 2025–2026 summer term.

**Status: M1–M4 implemented and verified (115 tests green across 13 suites; CLI + GUI
flows proven end-to-end) — M5 is the 9.13 submission deliverables, tracked in
[`docs/deliverables-checklist.md`](docs/deliverables-checklist.md).** Start reading at
[`docs/SPEC.md`](docs/SPEC.md) (Part A = overview; History keeps the real timeline).

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

## Test / gate commands

```bash
cargo fmt --check && cargo clippy -D warnings && cargo test   # 115 tests, 13 suites (fixture trees only)
cargo test --features llm                                    # 125 tests (adds the HTTP client suite)
```

## Milestones

| Milestone | Status | Evidence |
|---|---|---|
| M1 scan + classify core | ✅ | 9f4cb7c (dua-core 3.3.0 pin at the time — bumped to `=4.0.0` 09-10 per D-37; syscall tests, fixture generator) |
| M2 plan/execute + WAL + ledger | ✅ | 9fd33f1, 17647bb (crash injection, restore keep-both, non-UTF8 refusal) |
| M3 GUI shell | ✅ | 97d704f (Tauri 2.11.5, 17 IPC commands, writable-vol dedup) |
| M4 config + agent + cost | ✅ | 3ed9545, a671575, 77f0b97 (NL policy E2E 腾出 25 GB) |
| M5 submission deliverables | 🔄 | [`docs/deliverables-checklist.md`](docs/deliverables-checklist.md) |

## Documentation

| Doc | Contents |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | **single source of truth** — Parts A product · B technical · C UX · D dependencies, status & milestones |
| [`docs/design-decisions.md`](docs/design-decisions.md) | 17 ADRs + the D-01…D-31 decision register (appendix) |
| [`docs/research/`](docs/research/) | 33 verified research notes + `archive/` (competitor survey), cited as [NN] |
| [`docs/ai-logs/`](docs/ai-logs/) | per-milestone AI conversation excerpts (graded deliverable, redacted) |
| [`docs/expense-sheet.md`](docs/expense-sheet.md) | 开销明细表 — cost breakdown by phase (graded deliverable) |

## License / provenance

Course submission — pushed to the Tsinghua Git server; LICENSE decision deferred
until after grading (decision register D-07 — appendix of `docs/design-decisions.md`).
AI-assisted development throughout; conversation logs are a graded deliverable
(per-milestone export, `docs/ai-logs/`).
