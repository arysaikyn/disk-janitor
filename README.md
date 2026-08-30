# Disk Janitor 「还我磁盘」

A scenario-specialized, evidence-driven macOS disk-cleanup Agent — built as the course
project for 程序设计训练（Rust 语言）(30240522-3), Tsinghua CS, 2025–2026 summer term.

**Status: scaffold only — no implementation yet.**

## The idea in one line

`du` tells you *where* the bytes are; it never tells you whether they are *safe to
remove*. Disk Janitor classifies reclaimable space with a machine-derived evidence
chain, executes deletions as a reversible two-phase protocol (quarantine → purge),
and lets you undo everything for 72 hours. 删除是操作，恢复是权利。

## Why not just use an LLM chat?

- No rebuild-cost model: it can't reason "node_modules with a lockfile ≈ 40 s to
  reinstall" per directory.
- No persistent ledger: every session re-judges the same directories from zero.
- No batch atomicity: one `rm` at a time can't offer whole-batch rollback.
- It can't be held accountable for its own judgment, so it stops at "advice".

## Documentation

| Doc | Contents |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | product spec (draft v0) |
| [`docs/DESIGN.md`](docs/DESIGN.md) | build-ready technical design (architecture, modules, safety, testing) |
| [`docs/ecosystem-survey.md`](docs/ecosystem-survey.md) | tool landscape: what to copy, adapt, or reject (licenses verified) |
| [`docs/crate-map.md`](docs/crate-map.md) | dependency plan + proposed Cargo.toml block (versions verified 2026-08) |
| [`docs/design-decisions.md`](docs/design-decisions.md) | ADR log — the 10 decisions and their evidence |
| [`docs/research/`](docs/research/) | 8 research notes: primary-source evidence behind every claim |

## Core design (see `docs/SPEC.md` for details)

1. **Evidence-driven reclaimability engine** — candidates are graded by rebuild cost
   (pure cache / rebuildable from source / re-downloadable / expensive /
   looks-like-junk-but-hands-off), each verdict backed by inspectable evidence
   (lockfile present? package.json gone? mtime 10 min ago → a process is writing it).
2. **Two-phase safe execution** — `plan` (reviewable JSON, terraform-style) →
   `apply` (atomic same-volume move into quarantine via `renamex_np(RENAME_EXCL)`,
   zero bytes freed yet) → `purge` (typed numeric confirmation required) with a
   72 h grace window and `restore`; write-ahead log for crash recovery.
3. **Natural-language policy compiler** — "free 25 GB, don't touch projects I ran
   this week" compiles to structured constraints for the batch planner. The LLM
   (optional) never sees paths or filenames — intent and aggregate stats only.

## Agent rules for working in this repo

See `AGENTS.md` (also read by Claude Code via `CLAUDE.md` symlink).

## Build (once implementation starts)

```bash
cargo build          # debug build
cargo test           # unit + integration tests (fixture trees only)
cargo run -- scan ~/ # example
```

## License / provenance

Course submission — source will be pushed to the Tsinghua Git server. AI-assisted
development throughout; conversation logs are kept as a graded deliverable.
