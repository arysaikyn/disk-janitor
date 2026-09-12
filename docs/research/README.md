# Research Corpus — Index & Decision Sheet

Status: **complete 2026-09-13** · 33 notes · every claim license/version-verified or
explicitly flagged · consumed by `docs/SPEC.md` Parts A–D (citations `[NN §x]`).

## How to read this corpus

Notes are additive layers, not parallel essays. Read order for a newcomer:

1. `03` (what junk exists + evidence rules) → `04`/`05` (how to walk/read macOS safely)
2. `01`/`02` (what existing tools do + their safety models) → `08` (how to log it durably)
3. `09`–`11` (Windows/Linux deltas) → `12` (GUI stack) → `13` (product presentation)
4. `14`–`20` (product/UX decisions: space exploration, one-tap, protection, vault, errors,
   parity, brand/audit) → `21` (why the reference product feels good)
5. `22`–`33` (**borrow sweep for 技术选型** — one subsystem each; every note ends in a
   `## Port backlog` table naming an exact symbol in this repo). Independent of each other
   but additive over 01–21; read the one whose subsystem you are working on.

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
| 21 | Why DaisyDisk feels good | interaction-design principle extraction (read-only, no design decisions) | verified 09-07 |
| 22 | OSS fork viability | every candidate as fork base / component donor / reject + license virality | verified 09-13 |
| 23 | macOS syscall wrapper crates | Darwin FS primitives: what wraps them, what doesn't, why we hand-roll | verified 09-13 |
| 24 | Journal & crash-recovery mechanisms | framing, checksum, commit marker, torn-tail resync across 6 production journals | verified 09-13 |
| 25 | Trash → quarantine → purge semantics | move/restore/collision/purge functions and guard predicates across the OSS families | verified 09-13 |
| 26 | Parallel scan internals | hardlink dedup, size semantics, error policy in 10 walkers (incl. the two live bugs) | verified 09-13 |
| 27 | Artifact-classification rulesets | how production cleaners express a rule + derived-but-regenerable decision | verified 09-13 |
| 28 | Policy languages & size parsing | parse/eval splits, unrecognized-fragment reporting, size-unit grammar | verified 09-13 |
| 29 | Agent frameworks & data boundaries | which Rust agent frameworks have a redaction seam; ours as structural invariant | verified 09-13 |
| 30 | Typed IPC in Tauri 2 | tauri-specta vs hand-mirrored DTOs, `ipc::Channel<T>` internals, error wire-shapes | verified 09-13 |
| 31 | Destructive-action UX patterns | platform guidelines + shipping ceremonies across Review/PlanReview/PurgeGate/Ledger | verified 09-13 |
| 32 | Crash & fixture test harnesses | VOPR/redb/SQLite/sled crash-testing + property tests + fixture generation | verified 09-13 |
| 33 | Rust safety-crate shortlist | exact revisions, licenses, advisories for the safety-primitive dependency surface | verified 09-13 |

**How notes 22–33 relate to 01–21.** Notes 01–21 are *additive layers* answering "what does
this landscape look like and what should the product be". Notes 22–33 are a second, **code-level
borrow sweep** commissioned for the 技术选型 section: each takes one subsystem we already own
(a named symbol in `src/`, `src-tauri/` or `app/`) and asks what shipping OSS does *inside* that
mechanism, ending in a `## Port backlog` table of rows that name an exact destination symbol in
this repo. They are additive over 01–21 by construction — every note opens with an explicit
"NEW vs. already covered" statement — and they carry no design decisions. Three findings are
**verified defects in our own code**, reproduced rather than inferred: `src/policy.rs:249`'s
fraction-exponent panic (28), the two wrong BSD-flag constants in `src/classify/macos.rs:32-33`
(23, re-verified 33), and the hardlink double-count + `first_dev` multi-volume prune (26).
Reporting them is in scope for the corpus; fixing them is a code change and was out of scope
for this research pass.

Verification status: notes 01–13 verified against live sources on their research dates
(SPEC Part B header). Notes 15/17/19 carry a **recovery flag**: their agents died before
persisting vendor URLs; the claims they rest on are also present in notes 02/13's verified
source lists, but the six one-tap / Time-Machine / WizTree-family pages themselves are queued
for re-fetch when search infrastructure returns. Note 20's App-Store/USPTO/CNIPA checks are
genuinely OPEN (search TLS-down 2026-09-01).

## Decision sheet — open product questions + owner outcomes

Q1–Q3 were asked of the owner 2026-09-01 (structured picker); outcomes are recorded below
and landed in SPEC as D-23…D-25 (Round 4). Q4–Q8 were NOT re-asked — their evidence-backed
recommendations were landed as defaults (D-26…D-30) and remain owner-vetoable.

| # | Question | Options | Evidence | Recommended | Outcome |
|---|---|---|---|---|
| Q1 | Brand direction (EN name/identity) | A keep "Disk Janitor" · B coinage on undo promise (还我磁盘 stays CN; EN descriptor) · C evidence-led name | note 20 §1–3: name taken by 4 live products; CN name already is the promise | B | **B approved → D-23** (EN candidate picked 09-08: Giveback) |
| Q2 | One-tap clean auto-mark set | A PureCache only · B PureCache + Rebuildable (with confirm list) | note 15 §4–6: no competitor gates by grade; evidence-graded auto-actions must stay minimal | A (with awaiting-confirm count surfaced) | **B approved (overrides recommendation) → D-24** |
| Q3 | Vault placement | A tab inside Ledger (台账\|恢复站) · B separate screen | note 17 §6: vault = actionable subset of ledger | A | **A approved → D-25** |
| Q4 | Restore collision default | A rename (Keep Both) · B ask always | note 17 §5: Time Machine defaults to Keep Both; reverse-rename maps for free | A | default landed → D-26 (vetoable) |
| Q5 | One-tap typed-gate placement | A at purge (final typed total) · B per-batch | note 15 §2/risks + note 13 §G2: signature moment must not be weakened | A | default landed → D-27 (vetoable) |
| Q6 | Error catalog into SPEC? | A append as SPEC Part C appendix · B keep as research note only | note 18 §3: paste-ready table | A | default landed → D-28 (vetoable) |
| Q7 | Parity claims in demo copy | A use =/≈ wording with deltas in same breath · B claim all 3 platforms equal | note 19 risks #1 + SPEC Part A §8 honest-guarantee | A | default landed → D-30 (vetoable) |
| Q8 | Vault countdown clock | A ledger-recorded monotonic timestamps · B wall clock | note 17 risks #4: clock jumps must not coerce grace | A | default landed → D-29 (vetoable) |