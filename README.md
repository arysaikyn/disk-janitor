# Disk Janitor 「还我磁盘」

**A disk-cleanup app that tells you what's safe to delete, shows the evidence, and
lets you undo everything.** Native macOS/Windows/Linux desktop app (Tauri + Rust).
Course project for 程序设计训练（Rust 语言）(30240522-3), Tsinghua CS, 2025–2026
summer term — built as a real product first.

**Status: design complete (13 research notes, 17 ADRs, UX spec) — implementation
starts at M1.** Start reading at [`docs/OVERVIEW.md`](docs/OVERVIEW.md).

## Why this exists

`du` tells you *where* the bytes are; it never tells you whether they are *safe to
remove*. Every existing tool — DaisyDisk, CleanMyMac, dua-cli — stops at byte
locations or black-box verdicts. Disk Janitor:

1. **Judges with evidence, not names** — every candidate carries an inspectable
   evidence chain (lockfile present? sibling manifest gone? written 3 min ago →
   refuse) and a rebuild-cost grade (PureCache → HandsOff).
2. **Makes deletion reversible by default** — `apply` moves to an on-volume
   quarantine (atomic, zero bytes freed); `purge` requires typing the exact byte
   count; 72-hour `restore` is one command. 删除是操作，恢复是权利。
3. **Keeps memory** — an append-only JSONL ledger remembers every verdict and
   operation across sessions, so the tool never re-judges from zero.
4. **Uses AI without giving it authority** — the agent understands "腾出 25GB，别动
   我这两周在跑的项目" and proposes plans, but never deletes; it never even sees a
   file path (privacy chokepoint + CI leak test).

## Documentation

| Doc | Contents |
|---|---|
| [`docs/OVERVIEW.md`](docs/OVERVIEW.md) | **start here** — vision, architecture, status, doc map |
| [`docs/SPEC.md`](docs/SPEC.md) | product spec + R1–R6 rubric map |
| [`docs/UX-SPEC.md`](docs/UX-SPEC.md) | the app: screens, wireframes, flows, visual language |
| [`docs/DESIGN.md`](docs/DESIGN.md) | technical design: types, threads, syscalls, tests |
| [`docs/design-decisions.md`](docs/design-decisions.md) | 17 ADRs with owner verdicts |
| [`docs/crate-map.md`](docs/crate-map.md) | dependency plan (versions verified 2026-08) |
| [`docs/ecosystem-survey.md`](docs/ecosystem-survey.md) | competitor analysis: copy/adapt/reject |
| [`docs/decision-register.md`](docs/decision-register.md) | the 22 owner decisions behind the GUI pivot (08-31 grill) |
| [`docs/research/`](docs/research/) | 13 primary-source research notes |

## Build (once implementation starts)

```bash
cargo build          # Rust core (scan/classify/execute/ledger/agent)
cargo test           # unit + integration (fixture trees only, never real user data)
cargo tauri dev       # Tauri app (frontend; bun 1.3 present, npm/pnpm also work) — docs/UX-SPEC.md
```

## License / provenance

Course submission — pushed to the Tsinghua Git server; LICENSE decision deferred
until after grading (decision register D-07, `docs/decision-register.md`). AI-assisted development throughout;
conversation logs are a graded deliverable (per-milestone export, `docs/ai-logs/`).
