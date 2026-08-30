# CLAUDE.md — Disk Janitor agent rules

Course project for 程序设计训练（Rust 语言）2025–2026 夏季学期, THU CS (30240522-3).
Topic: 「还我磁盘 Disk Janitor」— a scenario-specialized macOS disk-cleanup Agent.
**Full spec: `docs/SPEC.md` — read it before proposing or writing anything.**

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

## Conventions

- Branches: `agent/<short-desc>`; conventional commits (`feat:`, `fix:`, `docs:`…).
  Bootstrap: the first commit lives on `agent/repo-scaffold`; `main` stays unborn
  until Arys blesses it (`git checkout -b main`). Never commit directly to `main`
  afterward without an approved PR or explicit say-so.
- Check gate before yielding: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.
- Modules mirror the spec: `classify/` (evidence engine), `plan/`, `execute/`
  (quarantine/restore), `ledger/` (JSONL), `policy/` (NL strategy compiler),
  `ui/` (ratatui), `agent/` (LLM loop). One concept per file.
- Chinese user-facing strings are fine (course audience is Chinese); keep identifiers
  and comments in English.

## Course deliverables to keep green

- Design doc (`docs/DESIGN.md` eventually) must stay in sync with implementation.
- AI conversation logs are a graded deliverable — this session's transcript is part
  of that; keep timestamps and don't fabricate history.
- R1–R6 are the grading rubric for 功能完整性 (30 pts). When implementing, map every
  feature to its R-number.
