# CLAUDE.md — Disk Janitor agent rules

Course project for 程序设计训练（Rust 语言）2025–2026 夏季学期, THU CS (30240522-3).
Topic: 「还我磁盘 Disk Janitor」— a disk-cleanup desktop App (Tauri + Svelte; Rust core),
native on macOS/Windows/Linux. **Everything lives in `docs/SPEC.md` (Parts A–D) — read it
before proposing or writing anything; its top (Part A) is the project overview.**

## Hard rules

1. **No code without Arys's explicit approval.** The repo is currently a bare scaffold;
   every implementation step requires a green light in chat first.
2. **Rust for core logic (course R1).** All scanning/classification/WAL/ledger logic in
   Rust. The agent loop, tool calls, and model orchestration live in Rust; never hand
   those to a shell script or another language.
3. **Safety is the product.** This tool moves (eventually purges) real user files.
   - Destructive ops only ever move into an on-volume quarantine dir via
     `renamex_np(RENAME_EXCL)` — never `unlink` in normal paths.
   - `purge` requires typing the exact byte count; `restore` must always work within
     the grace window; WAL must make crash recovery possible.
   - Default dry-run: mutating commands without explicit flags/confirmation are a bug.
4. **Never test destructive paths on real user data.** Tests run against fixture trees
   under `tests/fixtures/` or a tempdir, never `~/Library`, `~/Library/Caches`, etc.
   Synthetic test data only.
5. **Never commit secrets.** API keys live in `.env` / `config.local.toml` (gitignored).
   `.env.example` documents the shape. The spec requires the LLM never sees paths or
   filenames — keep it that way in any prompt/code touching the model.

## Session-start contract (read-time drift check)

A fresh session trusts the front door; a stale one propagates. Before ANY work:

1. **Freshness** — SPEC.md's status snapshot carries a date. If it predates the newest
   commit, or is over 2 days old, the docs are stale: diff `git log` since the snapshot
   and re-sync before proceeding.
2. **Counts agree** — note/ADR/decision counts appear in ≥4 places (SPEC snapshot,
   SPEC Where-we-are table, SPEC History, README status + doc table). Grep the exact
   count; if any copy disagrees, fix all before trusting any.
3. **One fact, one grep, one commit** — after any docs-state change (new note, decision,
   stamp, milestone): grep every occurrence of the changed fact, update all copies AND
   the project memory summary (`memory://root/memory_summary.md`) in the same pass,
   commit once. Never "update later".

## Conventions

- Branches: `agent/<short-desc>`; conventional commits (`feat:`, `fix:`, `docs:`…).
  Bootstrap: the first commit lives on `agent/repo-scaffold`; `main` stays unborn
  until Arys blesses it (`git checkout -b main`). Never commit directly to `main`
  afterward without an approved PR or explicit say-so.
- Check gate before yielding: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.
- Modules mirror the spec: `classify/` (evidence engine), `plan/`, `execute/`
  (quarantine/restore), `ledger/` (JSONL), `policy/` (NL strategy compiler),
  `agent/` (LLM loop), `platform/` (per-OS traits), `ui/` (Tauri+Svelte app in
  `app/`, thin CLI plumbing in the core bin). One concept per file.
- UI is a Tauri + Svelte desktop App (ADR-5/18; **SPEC.md Part C** governs) —
  NOT a TUI; the TUI stack is shelved. Chinese-primary + English-secondary
  bilingual labels (ADR-13); identifiers/comments in English.

## Course deliverables to keep green
- Read `docs/SPEC.md` Part A first for the current vision/architecture/status;
  it is the front door — keep it in sync when reality changes.
- Design docs (`docs/SPEC.md` Parts A–D, `docs/design-decisions.md`) must stay in
  sync with implementation — when code deviates from a doc, update the doc (or
  file an ADR) in the same change. Course rule.
- AI conversation logs are a graded deliverable — this session's transcript is part
  of that; keep timestamps and don't fabricate history.
- R1–R6 are the grading rubric for 功能完整性 (30 pts). When implementing, map every
  feature to its R-number.
