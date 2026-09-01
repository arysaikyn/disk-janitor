# Research Corpus — Index & Decision Sheet

Status: **complete 2026-09-01** · 20 notes · every claim license/version-verified or
explicitly flagged · consumed by `docs/SPEC.md` Parts A–D (citations `[NN §x]`).

## How to read this corpus

Notes are additive layers, not parallel essays. Read order for a newcomer:

1. `03` (what junk exists + evidence rules) → `04`/`05` (how to walk/read macOS safely)
2. `01`/`02` (what existing tools do + their safety models) → `08` (how to log it durably)
3. `09`–`11` (Windows/Linux deltas) → `12` (GUI stack) → `13` (product presentation)
4. `14`–`20` (product/UX decisions: space exploration, one-tap, protection, vault, errors,
   parity, brand/audit)

## Index

| # | Note | Scope | Status |
|---|---|---|---|
| 01 | Open-source disk analyzers | ncdu/gdu/dua-cli/dust/diskus/kondo walks, licenses | verified 08-30 |
| 02 | macOS cleanup landscape | CleanMyMac/DaisyDisk/AppCleaner/Pearcleaner safety models | verified 08-30 |
| 03 | Dev-junk taxonomy | per-ecosystem junk paths + evidence + regen cost | verified 08-30 |
| 04 | Rust walk & metadata crates | jwalk/ignore/walkdir/rayon comparisons | verified 08-30 |
| 05 | macOS FS edge cases | renamex_np, dataless, xattr, EXDEV, quarantine root | verified 08-30 |
| 06 | TUI stack (ratatui) | shelved by GUI pivot (ADR-5/18); kept as reference | verified 08-30 |
| 07 | LLM agent loop (Rust) | async-openai, wire format, token/cost accounting | verified 08-30 |
| 08 | Ledger + WAL crash safety | JSONL framing, F_FULLFSYNC, recovery, flock | verified 08-30 |
| 09 | Windows junk taxonomy | WinSxS/Installer/Storage Sense; gated-verbs | verified 08-30 |
| 10 | Linux junk taxonomy | pacman/apt caches, journald, verb delegation | verified 08-30 |
| 11 | Cross-platform primitives | renameat2/FileRenameInfoEx probes, fs matrix | verified 08-30 |
| 12 | Tauri + Svelte GUI stack | versions, ipc::Channel, packaging | verified 08-31 |
| 13 | Disk-tool product presentation | landing structure, demo video conventions | verified 08-31 |
| 14 | Space-discovery UX | treemap drill-down, d3-hierarchy/layerchart/matterviz | 09-01 (npm-verified) |
| 15 | One-tap clean + receipts | Smart Scan flows, receipt patterns, white space | 09-01 (vendor URLs re-verify queued) |
| 16 | Protected folders UX | DaisyDisk/CleanMyMac/Time Machine/ripgrep patterns | 09-01 (19 sources) |
| 17 | Quarantine vault + restore UX | Recently Deleted grammar, collision triad, vault spec | 09-01 (S1–S15 re-verify queued; §4–§7 corpus-grounded) |
| 18 | Failure & error catalog | E-#### rows, CN/EN copy, P1 shape | 09-01 (corpus-grounded) |
| 19 | Platform parity matrix | = / ≈ / − per feature per OS, packaging, honest claims | 09-01 (vendor facts queued re-verify) |
| 20 | Brand + course-audit deliverables | name collisions (live), naming directions, deliverables checklist | 09-01 (App Store/trademark OPEN — infra) |

Verification status: notes 01–13 verified against live sources on their research dates
(SPEC Part B header). Notes 15/17/19 carry a **recovery flag**: their agents died before
persisting vendor URLs; the claims they rest on are also present in notes 02/13's verified
source lists, but the six one-tap / Time-Machine / WizTree-family pages themselves are queued
for re-fetch when search infrastructure returns. Note 20's App-Store/USPTO/CNIPA checks are
genuinely OPEN (search TLS-down 2026-09-01).

## Decision sheet — open product questions needing owner answers

Every question below is asked in the session that produced this corpus. Recommended =
the option this corpus's evidence supports; SPEC lands the answer verbatim after approval.

| # | Question | Options | Evidence | Recommended |
|---|---|---|---|---|
| Q1 | Brand direction (EN name/identity) | A keep "Disk Janitor" · B coinage on undo promise (还我磁盘 stays CN; EN descriptor) · C evidence-led name | note 20 §1–3: name taken by 4 live products; CN name already is the promise | B |
| Q2 | One-tap clean auto-mark set | A PureCache only · B PureCache + Rebuildable (with confirm list) | note 15 §4–6: no competitor gates by grade; evidence-graded auto-actions must stay minimal | A (with awaiting-confirm count surfaced) |
| Q3 | Vault placement | A tab inside Ledger (台账\|恢复站) · B separate screen | note 17 §6: vault = actionable subset of ledger | A |
| Q4 | Restore collision default | A rename (Keep Both) · B ask always | note 17 §5: Time Machine defaults to Keep Both; reverse-rename maps for free | A |
| Q5 | One-tap typed-gate placement | A at purge (final typed total) · B per-batch | note 15 §2/risks + note 13 §G2: signature moment must not be weakened | A |
| Q6 | Error catalog into SPEC? | A append as SPEC Part C appendix · B keep as research note only | note 18 §3: paste-ready table | A |
| Q7 | Parity claims in demo copy | A use =/≈ wording with deltas in same breath · B claim all 3 platforms equal | note 19 risks #1 + SPEC Part A §8 honest-guarantee | A |
| Q8 | Vault countdown clock | A ledger-recorded monotonic timestamps · B wall clock | note 17 risks #4: clock jumps must not coerce grace | A |