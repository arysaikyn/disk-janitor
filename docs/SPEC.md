# Disk Janitor — SPEC (single source of truth)

> **One file, everything decided.** Vision → product → architecture → UX → dependencies → status.
> Absorbed 2026-08-31 (content moved verbatim, not summarized): `OVERVIEW.md`, `DESIGN.md`,
> `UX-SPEC.md`, `crate-map.md` → Parts A–D below. Decisions & history:
> `design-decisions.md` (17 ADRs + decision-register appendix).
> Course: 程序设计训练（Rust 语言）(30240522-3), THU CS. Keep this file in sync with the
> implementation — when code deviates, update the doc (or file an ADR) in the same change.

**Status snapshot (2026-08-31):** design 100% · implementation 0% (M1 awaits owner go) ·
5 ADRs (13/14/16/17/18) await owner line-review · 9/1 design comments: review & submit.
**Evidence base:** `research/` — 13 verified notes + the competitor survey (archive, immutable).

**Pre-merge citation map** (existing references stay valid):

| Old reference | Now |
|---|---|
| `OVERVIEW.md` | Part A (incl. History & Status) |
| `SPEC.md §N` | Part A §N |
| `SPEC Part B §N` | Part B §N |
| `SPEC Part C §N` | Part C §N |
| `SPEC Part D §N` | Part D §N |
| `decision-register.md` | `design-decisions.md` → Appendix |

## Contents

- **Part A — Product**
  ## What this is
  ## The 30-second architecture
  ## The five safety invariants (the product's spine)
  ## 1. Positioning
  ## 2. Pain-point analysis (why existing tools fail)
  ## 3. Scenario customizations (course mandate: ≥2)
  ## 4. Architecture sketch
  ## 5. R1–R6 mapping (功能完整性, 30 pts)
  ## 6. Milestones (course deadlines: 9/1 comments, 9/6 public demo, 9/8 tryouts, 9/10 presentation)
  ## 7. Open questions / risks (owner flagged these as gut-feel)
  ## 8. Explicit non-goals
  ## History
  ## Where we are (2026-08-31)
- **Part B — Technical design**
  ## 1. Overview
  ## 2. System architecture
  ## 3. Module designs
  ## 4. Data flow — one scenario end-to-end
  ## 5. Safety architecture
  ## 6. Key type definitions
  ## 7. Testing strategy
  ## 8. Build sequencing
  ## 9. Design references — what each module borrows
- **Part C — UX design**
  ## 1. Product identity
  ## 2. Information architecture
  ## 3. Screen-by-screen wireframes
  ## 4. Key flows
  ## 5. Interaction rules
  ## 6. Component inventory
  ## 7. Competitor-learned patterns (note 13)
- **Part D — Dependencies**
  ## 1. Crates table
  ## 2. Pitfalls table
  ## 3. Proposed [dependencies] block
  ## 4. Feature-gating strategy
  ## 5. Version risk register (churn before 2026-09-08)

# Part A — Product

## What this is

**Disk Janitor 「还我磁盘」** is a disk-cleanup app for people whose disks are full.
It is a **real product first** (native macOS/Windows/Linux desktop app) that doubles
as the course project for 程序设计训练（Rust 语言）(30240522-3), Tsinghua CS.

### The vision in three sentences

Every existing cleanup tool — `du`, DaisyDisk, CleanMyMac, dua-cli — stops at
"here are the bytes." None of them answers the two questions that actually matter:
**"is this safe to delete, and why?"** (judgment) and **"what if I'm wrong?"**
(reversibility). Disk Janitor answers both: every deletion candidate carries a
**machine-derived evidence chain** the user can inspect (lockfile present, sibling
manifest gone, written 3 minutes ago → refuse), and nothing is ever really deleted
until you **type the exact byte count** — everything sits in a crash-safe quarantine
for 72 hours, restorable with one command. The AI agent understands intent
("腾出 25GB，别动我这两周在跑的项目") and proposes plans, but **never deletes** —
local code holds every authority, and the LLM never even sees a file path.

### The form

A **frictionless desktop app** (Tauri + Svelte; Apple-native feel, zero terminal
knowledge required) on a **Rust core** that does all scanning, judgment, and
execution natively on macOS, Windows, and Linux. Bilingual UI (中文主 + English
secondary). Zero telemetry. The full safety/UX contract lives in
[`docs/SPEC Part C`](SPEC Part C).

## The 30-second architecture

```
┌────────────────────────── Tauri + Svelte App (SPEC Part C) ──────────────────────┐
│  Dashboard · Scan · Review (evidence!) · Plan · Purge ceremony · Ledger · Agent │
└───────────────────────────────────┬─────────────────────────────────────────────┘
                                    │ Tauri IPC (typed channels)
┌───────────────────────────────────┴─────────────────────────────────────────────┐
│  RUST CORE (one crate, per-OS via platform/ traits)                             │
│                                                                                 │
│  scan/      parallel metadata-first walker (dua-core on macOS; statx/Win fast   │
│             paths elsewhere)          classify/  evidence engine → 5 grades     │
│  plan/      reviewable JSON manifest  execute/   quarantine (atomic per-OS)     │
│                                       purge / restore + WAL crash recovery      │
│  ledger/    append-only JSONL 台账 (every verdict & op, cross-session memory)   │
│  policy/    NL intent → structured constraints (local rules; LLM optional)      │
│  agent/     LLM loop — PROPOSE-ONLY, path-blind (privacy chokepoint + CI test)  │
│  config/    config.toml: endpoint, key, model, prices, budget                   │
│  platform/  AtomicMove · Walker · VolumeInfo · DatalessCheck traits per OS      │
└─────────────────────────────────────────────────────────────────────────────────┘
      thin CLI subset (scan --json / plan / restore) for CI + automation (ADR-14)
```

## The five safety invariants (the product's spine)

1. **Evidence over names** — a directory named `node_modules` is a *candidate*, never
   a verdict; every grade cites its evidence.
2. **Reversibility is the default state** — apply frees zero bytes; purge is the only
   irreversible step, gated by a typed byte count; no `--yes` bypass exists (P4).
3. **The LLM advises; local code decides** — agent is propose-only; paths never
   reach the model (P2, ADR-7).
4. **Honest bytes** — allocated size with hardlink/clone/sparse discounts; honest
   per-OS guarantees (Windows tiers stated, never faked) (P6).
5. **The ledger is the audit trail** — append-only, user-inspectable, never silently
   deleted; uninstall restores-or-confirms every quarantined file first (P5, ADR-16).

## 1. Positioning

A disk-cleanup **desktop App** (Tauri + Svelte; Rust core) — a real product first, native on
macOS, Windows, and Linux. It answers
the question `du` can't: not "where are the bytes?" but "**which of these are safe to reclaim,
why, and how do I get them back if I'm wrong?**" (macOS design researched first [04][05];
Windows/Linux taxonomies and cross-platform FS primitives in notes 09–11.)

- Users: everyone with a full disk — the GUI targets non-technical users first (no terminal
  knowledge required [ADR-18]); first users are developers on capacity-starved machines
  (Arys: 16 GB MacBook 飘红常客; classmates with 512 GB drives on 2-week cleaning cycles).
  An embedded **propose-only agent** (intent → plan, never deletes, path-blind) is a core
  differentiator over every surveyed tool [SPEC §3.3].
- Boundary: it only services the disk-cleanup scenario. It is not a general file
  manager and not a generic chat wrapper.

## 2. Pain-point analysis (why existing tools fail)

| Tool | What it gives | What it lacks |
|---|---|---|
| `du` / `ncdu` | byte locations | no safety judgment; no memory between runs |
| DaisyDisk | pretty maps | undo is best-effort at best — its "collect & delete" model is permanent-unlink (collector emptied on quit); nothing guarantees reversibility |
| CleanMyMac | "Safety Database" | black-box, human-curated rules; no lockfile/rebuild reasoning |
| dua-cli | newest-dir detection | officially "naming-based, deliberately conservative" |
| Generic LLM agent | knows DerivedData is deletable | won't act (irreversible), no persistent ledger, re-judges weekly, can't batch-rollback |

Core claim: **judgment ("can this be deleted?") and reversibility ("what if I'm
wrong?") are 99% of cleanup work; no existing tool owns either end-to-end.**

## 3. Scenario customizations (course mandate: ≥2)

1. **Evidence-driven reclaimability engine** — classification from evidence chains,
   not directory names. Every candidate gets a rebuild-cost grade
   (`PureCache / RebuildableFromSource / Redownloadable / Expensive / HandsOff`)
   plus machine-derived evidence the user can inspect
   (`node_modules` + valid lockfile + registry reachable → rebuildable ≈ 40 s;
   sibling `package.json` absent → orphan tree, promoted;
   `~/Library/Caches` entry with mtime < 10 min → actively written, refuse auto-action).
2. **Two-phase safe execution protocol** — `plan` → `apply` → `purge` → `restore`:
   - `plan`: reviewable JSON manifest (terraform-plan style).
   - `apply`: atomic same-volume move into an on-volume quarantine dir (macOS
     `renamex_np(RENAME_EXCL)`; per-OS equivalents behind the `platform/` layer — §4/§8)
     — zero bytes freed, fully reversible.
   - `purge`: actually frees space; requires typing the exact byte count.
   - `restore`: one command, full restoration within the 72 h grace window.
   - WAL (write-ahead log) ⇒ crash-recoverable; 删除是操作，恢复是权利.
3. **(bonus) NL policy compiler** — "腾出 25GB，别动我这两周在跑的项目" compiles to
   structured planner constraints. **Privacy invariant: the LLM never sees file
   paths or names — only the intent sentence and aggregate statistics.**

## 4. Architecture sketch

```
  scan/      walker (parallel, metadata-first: size/mtime/flags; dua-core on macOS,
             portable ignore::WalkParallel baseline elsewhere [10][11])
  classify/  evidence engine + rebuild-cost grading (rule engine, local)
  plan/      candidate selection under policy constraints → JSON manifest
  execute/   quarantine / purge / restore, WAL (syscall via platform/)
  platform/  per-OS abstraction: AtomicMove / Walker / VolumeInfo / DatalessCheck traits
             with #[cfg]-gated impls per OS (macOS renamex_np; Linux renameat2 + fs probe;
             Windows FileRenameInfoEx) [09][10][11]
  ledger/    persistent JSONL 台账: every verdict & operation, cross-session memory
  policy/    NL → structured constraints (local rules; optional LLM)
  agent/     agent loop: model ↔ tools (scan/classify/plan/apply), stop conditions,
             token budget enforcement
  ui/        Tauri + Svelte desktop App (SPEC Part C): evidence review, progress,
             ledger trajectory; thin CLI subset as plumbing
  config/    config.toml: endpoint, key, model, context length, thinking mode,
             API prices, budget
```

Key types (working names): `Candidate { path, size, class, evidence: Vec<Evidence> }`,
`Evidence { kind, detail, strength }`, `PlanEntry`, `LedgerRecord`, `WalEntry`.

## 5. R1–R6 mapping (功能完整性, 30 pts)

| Req | Implementation |
|---|---|
| R1 Rust core | all of the above; syscall layer via `nix`/`libc` |
| R2 UI | Tauri + Svelte 桌面 App (course R2 permits 桌面 App; SPEC Part C); thin CLI plumbing |
| R3 model config | `config.toml` + App settings pane: endpoint, key, model, ctx len, thinking, prices |
| R4 progress + interrupt | live scan/classify progress (ipc::Channel); interrupt-safe (WAL-consistent; apply auto-resumes, purge runs to completion) |
| R5 context/history | JSONL ledger + session save/load; agent-loop trajectory visible in App (Ledger screen) |
| R6 tokens & cost | per-call usage captured → running cost at configured prices; budget ⇒ interrupt |

## 6. Milestones (course deadlines: 9/1 comments, 9/6 public demo, 9/8 tryouts, 9/10 presentation)

- M0 scaffold — repo, agent files, spec (this commit).
- M1 scan + classify core w/ fixture-tree tests.
- M2 plan/apply/purge/restore + WAL + ledger.
- M3 GUI shell per SPEC Part C (Tauri + Svelte): evidence review, progress, interrupt.
- M4 agent loop + NL policy compiler + token accounting.
- M5 demo script, README polish, design doc final, AI-log export, 开销明细表.

## 7. Open questions / risks (owner flagged these as gut-feel)

- Classification thresholds (e.g. "mtime < 10 min ⇒ actively written") — need tuning.
- Grace period: is 72 h right?
- APFS snapshot interactions; iCloud placeholder files (dataless) — scan must not
  trigger evict/download; classify must mark them appropriately. Per-platform analogs:
  OneDrive `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` placeholders on Windows; none on Linux
  (NFS/autofs/overlayfs are the nearest analogues) [09 §3][11 §3].
- External volumes: quarantine must stay same-volume to be atomic, but "same-volume" is
  per-OS (macOS `st_dev`; Windows `dwVolumeSerialNumber`; Linux mount resolution via
  `/proc/self/mountinfo`), and atomic-no-clobber support itself is a per-fs/per-OS matrix
  (Linux `renameat2` NFS gap; Windows SMB; exFAT no `RENAME_EXCL` on macOS) — plan must
  probe and refuse with evidence where unsupported [05 §1][09 §0][10 §0][11 §1/§4].

## 8. Explicit non-goals
Cross-platform scope is a goal (§1); what stays out: generic file search; content indexing;
auto-delete daemon (monitoring/reminder maybe later per classmate feedback). What is NOT
promised is identical atomicity on every platform — the honest guarantee: (D-30:
parity claims in demo/packaging copy use the =/≈ delta language of note 19 §3, deltas
documented in the same breath, never a flat "3-platform" claim)

| Platform | Atomic no-clobber quarantine move | Caveats |
|---|---|---|
| macOS | full atomicity — `renamex_np(RENAME_EXCL)` [05 §1] | exFAT/FAT lack `VOL_CAP_INT_RENAME_EXCL` (probe + refuse) [05 §1] |
| Linux | atomicity on kernels ≥ 3.15 with `renameat2(RENAME_NOREPLACE)`, fs-dependent (ext4 ≥3.15, btrfs/tmpfs ≥3.17, xfs ≥4.0, many more ≥4.9) [10 §0][11 §1] | NFS rejects `RENAME_NOREPLACE` (`EINVAL`) — probe per volume, fall back or refuse [11 §1/risks] |
| Windows | POSIX-semantics rename via `SetFileInformationByHandle(FileRenameInfoEx)` (Win10 1709+; 1607 headers) [09 §delta][11 §1] | no explicit min-version in [MS-FSCC] — runtime probe, `MoveFileExW` fallback is weaker (non-atomic w.r.t. open targets); no dir-fsync (NTFS journal) [09 §delta][11 §1/§2] |

## History

Conceived 2026-08-27 from the course topic post; scaffold 08-30
(agent files, spec); a 13-note verified research corpus and 12 technical ADRs landed
08-30 (`7a91240`). On 08-30 the owner expanded scope to **native 3-platform** (notes
09–11, ADR-11/12, `102fef2`). A 3-round grill on 08-31 surfaced 28 design gaps and
produced 22 owner decisions — the pivotal one: **TUI → GUI-first Tauri desktop App**
(UX-SPEC, ADR-5/13/14/16/17/18, `8308fcf`). Honesty-pass fixes landed same day
(`abc23f2`, `88b8dbe`); overview + entry-point sync `f0fcaa6`. A second research wave
landed 09-01 (notes 14–20: space-discovery UX, one-tap receipts, protected folders,
vault/restore UX, failure catalog, platform parity, brand/audit) with owner decisions
D-23…D-30 (brand coinage, one-tap auto-mark set, vault tab, collision default, typed-gate
placement, error-catalog appendix, parity copy, countdown clock). Implementation (M1)
awaits the owner's go.

## Where we are (2026-08-31)

| Phase | Status |
|---|---|
| Research (20 notes) | ✅ done — 01–13 verified at source; 14–20 landed 09-01 (15/17/19 vendor-URL re-verify queued, note-flagged) |
| Architecture (17 ADRs) | ✅ 12 owner-stamped · ⏳ 5 awaiting owner line-review (13/14/16/17/18) |
| UI/UX design | ✅ UX-SPEC complete (wireframes, flows, purge ceremony) · ⏳ visual mockups open |
| **Implementation** | ❌ **0% — starts at M1 on owner's go** |
| Demo video + 9/10 presentation | ❌ not started (competitor research ready, note 13) |
| **9/1 design comments** | ⏳ docs complete as of 2026-08-31 — owner review & submit **by 9/1** |


# Part B — Technical design

> Build-ready architecture, 2026-08-30 (GUI-amended 08-31). Sources: `docs/SPEC.md` (draft v0)
> + research notes `docs/research/02…13-*.md` (02–08 verified 2026-08-30; 09–11 the Windows/
> Linux/cross-platform-primitives notes behind the 3-platform amendment; 12–13 the Tauri-stack
> and product-presentation notes behind the GUI pivot), cited inline as `[NN §x]`.
> Layout = single Rust crate `disk_janitor` (lib + bin); modules per SPEC Part A §4.
> Integration tests link the lib; no runtime besides per-thread runtimes (§2).

## 1. Overview

**Product claim.** Disk Janitor is a disk-cleanup desktop App (Tauri + Svelte; propose-only
agent embedded), native on macOS,
Windows, and Linux, that answers the one question `du` can't: not "where are the bytes?" but
"which of these are safe to reclaim, why, and how do I get them back if I'm wrong?" [SPEC Part A §1].
The macOS design was researched first ([02], [04], [05]); the Windows and Linux taxonomies
([09], [10]) and the cross-platform FS primitives ([11]) extend it — the judgment/reversibility
architecture carries over unchanged on all three, only the syscall substrate differs [11 §8].
The two halves of that claim
are exactly what no surveyed tool owns end-to-end [02 §2]. Judgment exists today only as
black-box or name-based heuristics: CleanMyMac's proprietary Safety Database is not
user-inspectable [02 §1], and dua-cli is deliberately "naming-based, conservative" [SPEC Part A §2].
Reversibility ends at the Trash (AppCleaner/Hazel: survives only until the Trash is emptied) or
does not exist at all — DaisyDisk's Collector staging ends in *permanent* unlink, "cannot be
'undeleted'" [02 §1 ⚠ correction to SPEC's pain table]. A generic LLM agent knows DerivedData is
deletable but won't act: it cannot undo, keeps no persistent ledger, and re-judges the same tree
every week [SPEC Part A §2]. Disk Janitor therefore ships: evidence-graded candidates, a reviewable
plan manifest, same-volume quarantine with WAL-guaranteed one-command restore, a cross-session
JSONL ledger, and an optional LLM that compiles intent into policy and never sees a path
[SPEC Part A §3].

**Design philosophy: judgment + reversibility.** Three commitments shape every module:

1. *Evidence over names.* Every candidate carries machine-derived evidence the user can inspect
   (lockfile presence, sibling-manifest absence, subtree mtime heat, flock markers,
   content-addressable-store shape, `st_nlink`/APFS clone-id, `com.apple.quarantine` xattr) and
   a rebuild-cost grade `PureCache / RebuildableFromSource / Redownloadable / Expensive /
   HandsOff` with a regen command and measured cost [03 §0, §1]. Name matching is precisely the
   failure mode we supersede (AppCleaner's false-positive leftovers) [02 §4].
2. *Reversibility as the default state.* `apply` frees zero bytes: `renamex_np(RENAME_EXCL)`
   moves subtrees into a hidden, per-volume quarantine directory [05 §7]. 删除是操作，恢复是权利 —
   deletion is an operation, restoration is a right. The only irreversible step (`purge`) is
   gated by a typed byte count; WAL + checksummed JSONL make every state crash-recoverable
   [08 §4–5].
3. *Honesty — in bytes and in privacy.* Byte counts use allocated size with hardlink / clone /
   sparse discounts (`st_size` lies for pnpm stores, Docker.raw, APFS-compressed files)
   [03 §6, §8-risks; 05 §6]; freed-space reports account for Time-Machine snapshot pinning
   [02 §3]; and the LLM layer is *architecturally* unable to see paths — enforcement lives in
   one serializer chokepoint plus a CI leak test, not in prompt discipline [07 §privacy].
4. *Non-functional floor (D-13/D-14).* Scan sustains ≥1 GiB/s metadata-only on M1 Pro — a
   home-dir scan completes in <2 min and is bench-gated in CI; the whole process stays
   under a 500 MB RSS ceiling, asserted against a 1M-entry synthetic fixture.

## 2. System architecture

```
                 +------------------------------------------------------------+
                 |                 disk-janitor (one process)                 |
                 |                                                            |
 keyboard ---> [Input thread] --input_rx(32)---------------------------+     |
                 |                                                     |     |
                 |   scan_rx(100)                     +----------------v---+ |
 [Scan thread] --+----------------------------------->|     UI thread      | |
   owns dua-core |   cand_rx(1000)                    | Terminal + AppState| |
   RootWalk iter +----------------------------------->| select! -> draw    | |
   (drop=cancel) |   exec_rx(16)                      +--------------------+ |
                 +----------------------------------->    ^         ^  ^     |
                 |   agent_rx (turns, receipts, trajectory)  |         |       |
 [Exec thread] --+-------------------------------------------+         |       |
   apply/purge/  |                                                     |       |
   restore       |                                    [Agent thread] --+       |
                 |                                     own tokio rt;          |
 [Tick thread] --+-- tick_rx(1) 100 ms AtomicBool flip  async-openai;         |
                 |                                      sans-IO AgentRun;     |
                 |                                      tools call local      |
                 |                                      scan/classify/plan/   |
                 |                                      execute in-process    |
                 |                                                            |
                 |  [Walker pool] dua-core work-steal, n=available_parallelism|
                 |    spawned by Scan thread; DirStat batches -> scan_rx      |
                 |                                                            |
                 |  Durable state (files, not threads):                       |
                 |    ~/.disk-janitor/{ledger.jsonl, wal.jsonl}  (flock)      |
                 |    <vol>/.Trash-janitor/<op>/ + per-volume WAL             |
                 |    plan-<ts>.json manifest                                 |
                 +------------------------------------------------------------+
```

### 2.1 Thread model

| Thread | Owns | Emits | Consumes | Notes |
|---|---|---|---|---|
| Input | `crossterm::event::read()` loop | `input_rx` | — | bounded(32); dua's exact pattern [06 §5] |
| UI | `Terminal<CrosstermBackend>`, `AppState` | `cmd_tx`, draws | all `*_rx` | redraw per event (diffs are cheap) + 100 ms tick during long tasks; ncdu/gdu cadence [06 §5] |
| Scan | dua-core `RootWalk`, dataless I/O policy | `scan_rx` | cancel flag | dropping the iterator cancels + joins workers [04 §dua] |
| Walker pool | (dua-core internal) | → Scan thread | — | work-stealing, `available_parallelism()` threads [04 §stack] |
| Classify | rayon pool | `cand_rx` | `scan_rx`, cancel | evidence IO per candidate; no shared maps, channel boundary [04 §6] |
| Exec | WAL fd, ledger store, quarantine dirs | `exec_rx` | plan, cancel | strictly sequential, WAL-ordered; polls cancel between entries |
| Agent | own tokio rt, `ChatClient` | `agent_rx` | `cmd_tx`, tools | sans-IO `AgentRun` stepped on this thread [07 §rig] |
| Tick | `AtomicBool` | `tick_rx` | — | flips every 100 ms (dua `Throttle`) [04 §progress] |

No tokio outside the agent thread — dua and gitui stay on threads + crossbeam, and ratatui's FAQ
agrees async only pays off with many concurrent tasks [06 §5].

### 2.2 Channel topology

| Channel | Kind (crossbeam-channel) | Bound | Backpressure / drop policy |
|---|---|---|---|
| `input_rx` | `crossterm Event` | 32 | `try_send`; drop key-repeat spam on Full (dua accepts Full drops) [06 risks] |
| `scan_rx` | `ScanEvent` (batched `DirStat`s) | 100 | dua's cap; scanner blocks, UI drains `try_recv` [04 §progress] |
| `cand_rx` | `Candidate` | 1000 | classify blocks |
| `exec_rx` | `ExecEvent` | 16 | exec blocks (user-visible ops are slow anyway) |
| `agent_rx` | `AgentEvent` | unbounded (low rate) | turns are seconds apart |
| `tick_rx` | `()` | 1 | `try_send`, latest-wins |
| `cmd_tx` (UI →) | `UiCmd` | 8 | blocking send — user intent is never dropped |

Shared state: `Arc<AtomicBool>` cancel (checked per directory / candidate / plan entry) and
`Arc<ScanStats>` atomics (entries, bytes, elapsed) for the footer. Nothing else — no
`DashMap`, no cross-thread locks [04 §6].

### 2.3 Process-level invariants

- `setiopolicy_np(IOPOL_MATERIALIZE_DATALESS_FILES_OFF)` at scan start (saved/restored via
  `getiopolicy_np`); scanning iCloud trees never triggers downloads; content reads fail
  `EDEADLK` — fine, scan is metadata-only [05 §5][04 risks].
- `flock(LOCK_EX | LOCK_NB)` on `~/.disk-janitor/janitor.lock` — a second instance fails fast
  with a clear message [08 §6].
- Async confined to the agent thread; UI, scan, classify, exec are plain threads over
  crossbeam channels [06 §5].

### 2.4 Crates by layer

dua-core 3.3.0, crossbeam-channel, rayon, sysinfo, nix 0.31.3 + libc 0.2.189, rustix (xattr),
indicatif (headless CLI only), async-openai 0.41.3 + tokio (agent thread only), tiktoken-rs,
serde / serde_json / toml, crc32fast, tempfile 3.27.0, fs4, fs-err, same-file, dirs, secrecy.
The Tauri + Svelte GUI layer (per SPEC Part C, ADR-5/18) lives in `src-tauri/` + `app/` —
ratatui/crossterm/unicode-width/signal-hook are SHELVED (GUI pivot) and appear only in
SPEC Part D §1c for history.

Per-platform additions behind `#[cfg]` gates ([09][10][11]): Windows — `windows` 0.62.2 or
`windows-sys` 0.61.2 (FileRenameInfoEx / FlushFileBuffers / bulk enumeration), `winreg` 0.56.0;
Linux — `rustix` `fs::renameat_with`/`syncfs`/`statx`, `libc` `renameat2` + `RENAME_NOREPLACE`;
portable — `same-file` 1.0.6 (volume/file-index identity), `dirs` 6.0.0 (XDG/FOLDERID),
`filetime` 0.2.29 (mtime preservation). Full version/license table: SPEC Part D §1f.

## 3. Module designs

For each module: responsibility, key types, public API sketch, crates, and the research that
justifies it. Fields are the contract; method bodies elided.

### 3.1 `scan/` — parallel metadata-first walker

| Aspect | Decision |
|---|---|
| Responsibility | walk chosen roots, aggregate per-dir stats, emit batches; never materialize dataless files; never follow symlinks; stay on one filesystem by default; report volume truth (raw free vs purgeable) |
| Walker | dua-core 3.3.0 `walk_roots(roots, threads, Order::ParentFirst, opts, descend)` — metadata arrives with enumeration via `getattrlistbulk(2)` (one syscall per 64 KiB buffer; the macOS module requests mtime/flags/nlink/allocsize/clone-id in-bulk) [04 §dua] |
| Mount skipping | per-entry `MetadataExt::dev()` compare inside `descend`, not mount-table parsing; firmlink-safe — firmlinked `/`, `/Users`, `/System/Volumes/Data/...` share `st_dev` [05 §1][04 §4] |
| Sizes | capture both apparent (`ATTR_FILE_DATALENGTH`/`len`) and allocated (`ATTR_FILE_DATAALLOCSIZE`/`st_blocks·512`); allocated is what purge actually frees and what the typed confirm shows [04 §5][05 §6] |
| Dataless | read `SF_DATALESS` from `st_flags`/`getattrlist`; scan never opens content (reads fail `EDEADLK` under the OFF policy — fine, metadata-only) [05 §5] |
| Snapshots | record `tmutil listlocalsnapshots` per scan for freed-space honesty [02 §3] |
| Platform note | macOS keeps the dua-core `getattrlistbulk` walker as designed [04 §dua]. Portable baseline everywhere: `ignore` 0.4.33 `WalkParallel` (the only walker fully portable today [11 §5]); per-OS fast paths behind the same `DirStat` output — Linux `statx` birth-time/mtime via rustix [11 §7], Windows `GetFileInformationByHandleEx(FileIdBothDirInfo)` bulk enumeration copied from dua-core's windows module [11 §5]. Linux adds mount resolution per candidate via `/proc/self/mountinfo` (fstype → `renameat2` availability), not just `st_dev` [10 §0][11 §4]. Windows: never follow reparse points/junctions; OneDrive subtrees stat-only; `\\?\` paths past 260 chars [09 §0][09 §3]. |

```rust
pub struct ScanOptions { pub roots: Vec<PathBuf>, pub threads: usize,
    pub cross_filesystems: bool /* default false */, pub ignore: GlobSet,
    pub min_dir_bytes: u64 /* pre-filter before classify */ }
pub struct DirStat { pub dev: u64, pub ino: u64, pub parent: Arc<PathBuf>,
    pub name: OsString, pub depth: u16, pub kind: Kind,        // Dir|File|Symlink
    pub app_bytes: u64, pub alloc_bytes: u64, pub entries: u32,
    pub mtime: SystemTime, pub flags: u32 /* UF_/SF_ */, pub nlink: u32,
    pub clone_id: Option<u64> /* APFS copy-on-write clone [04 §dua] */ }
pub enum ScanEvent { Batch(Vec<DirStat>), RootError { root: PathBuf, err: io::Error },
    Finished(ScanTotals) }
pub struct ScanTotals { pub files: u64, pub alloc: u64, pub elapsed: Duration,
    pub snapshots: Vec<String>, pub volumes: Vec<VolumeInfo> }
pub struct VolumeInfo { pub dev: u64, pub mount: PathBuf, pub fs: String,
    pub total: u64, pub free_raw: u64 /* statfs */, pub free_important: u64,
    pub free_opportunistic: u64, pub read_only: bool, pub rename_excl: bool }

pub fn start(opts: ScanOptions, tx: Sender<ScanEvent>, cancel: Arc<AtomicBool>,
             stats: Arc<ScanStats>) -> thread::JoinHandle<ScanTotals>;
pub fn volumes() -> Vec<VolumeInfo>; // sysinfo Disks + CFURL capacity keys; sysinfo prefers
                                     // ImportantUsage on macOS [04 §sysinfo]
```

Justification: parallel directory-level traversal is the win; per-file `lstat` is eliminated by
bulk enumeration [04 §1]; jwalk is dead (README redirects to dua-core) so dua-core it is
[04 §stack]; `drop(iterator)` = cancel + join is exactly R4's interrupt primitive [04 §progress];
`SF_DATALESS` is a read-only synthetic flag — scanning never triggers materialization [05 §5];
`statfs`/`NSFileSystemFreeSize` excludes purgeable, so the UI surfaces both numbers, labeled
[02 risks #1][02 §3]; EXDEV is empirically raised even across APFS volumes in one container, so
same-`st_dev` is the real precondition [05 §2].

### 3.2 `classify/` — evidence engine + grading

Responsibility: map `DirStat` subtrees → `Candidate`s. Local-only, deterministic. Two layers:
(1) path-class rules recognize known junk shapes (per-ecosystem tables [03 §1–8]); (2) evidence
probes confirm, refute, or regrade. Every verdict carries ≥1 strong signal [03 §0].

Evidence signal table (all from [03 §0]):

| Signal | Read | Effect |
|---|---|---|
| `LockfilePresent` | package-lock.json / pnpm-lock.yaml / Cargo.lock / uv.lock … newer than or matching dir mtime | deterministic regen ⇒ `RebuildableFromSource`; enables timed regen-cost estimate |
| `SiblingManifestMissing` (orphan) | no manifest in any ancestor (node_modules w/o package.json; .venv w/o pyproject.toml) | cannot re-pin deps ⇒ surface as "project deleted?" prompt, never auto-delete |
| `MtimeHot` | subtree max mtime < 10 min | refuse auto-action; gates file-level deletion, *not* whole-store prune commands [03 risks #7] |
| `FileLock` | flock marker `.package-lock.json`, pnpm/uv store locks; `lsof`-equivalent fallback | `HandsOff` until process exits |
| `GitRepoAncestor` | `.git` ancestor | confirms rebuild-from-source; `.git/HEAD` mtime = project activity |
| `CasStore` | hash-named children (cacache `content-v2/sha512`, pnpm `files/`, uv `archive-v0`) | whole-store prune only — uv: "never safe to modify the cache directly" [03 §3] |
| `Hardlinked` / `ApfsClone` | `nlink>1` or `ATTR_CMNEXT_CLONEID` | byte discount: deleting one of N links frees 0 [03 §0] |
| `Dataless` / flag-block | `SF_DATALESS`; `UF_IMMUTABLE`/`SF_RESTRICTED`/`UF_DATAVAULT`/`SF_NOUNLINK` | `HandsOff`; rename/purge would `EPERM`/`EDEADLK` [05 §5][05 §6] |
| `QuarantineXattr` | `com.apple.quarantine` present | downloaded-by-app ⇒ `Redownloadable` evidence [05 §6] |

```rust
pub struct Thresholds { pub hot_mtime: Duration, pub ancestor_depth: u8, pub min_bytes: u64 }
pub fn run(rx: Receiver<ScanEvent>, tx: Sender<Candidate>, cancel: Arc<AtomicBool>,
           th: Thresholds) -> thread::JoinHandle<ClassifyReport>;
fn probe_ancestry(path: &Path, depth_cap: u8) -> Ancestry; // manifests, lockfiles, .git,
                                                           // pyvenv.cfg, .venv markers
fn classify_dir(d: &DirStat, anc: &Ancestry, th: &Thresholds) -> Option<Candidate>;
fn regen_for(kind: PathClass) -> Option<Regen>;             // "npm ci"≈40s, "cargo build"…
```

Path-class rule sources: node [03 §1], rust [03 §2] (cargo-sweep's toolchain-keyed `target/`
rules — MIT but unmaintained: borrow the rules, not the maintenance expectation [03 §2]),
python [03 §3], Xcode [03 §4] (resolve `IDECustomDerivedDataLocation` before trusting the
default path), Homebrew [03 §5] (invoke `brew cleanup`, don't re-implement its logic), Docker
[03 §6] (sparse Docker.raw ⇒ `HandsOff` as a file, propose `docker system prune`), browsers
[03 §7], generic caches [03 §8].

Crates: rayon 1.12 (CPU-bound classify only — `par_bridge` carries a documented IO-iterator
bottleneck warning [04]); rustix `fs::xattr` or libc `getxattr` [05]; cargo_metadata 0.23.1 to
enumerate Rust workspace `target/` dirs correctly [03 §rec]; globset for user ignore globs
[04].

### 3.3 `plan/` — selection + manifest

Responsibility: filter candidates under `PolicyConstraints` → a reviewable JSON manifest
(terraform-plan style); zero side effects; pre-flight refusals so `apply` never surprises.

| Rule | Behavior |
|---|---|
| grades allowed | default allow `{PureCache, RebuildableFromSource, Redownloadable}`; `Expensive`/`HandsOff` require explicit per-entry opt-in |
| hard refusals (evidence-backed) | `MtimeHot`, `FileLock`, `OrphanTree`, `Dataless`, `UF_IMMUTABLE`, `SF_RESTRICTED`, read-only volume |
| environment refusals | cross-volume target (`EXDEV` [05 §2]); no `VOL_CAP_INT_RENAME_EXCL` (`ENOTSUP` on exFAT/FAT [05 §1]); non-APFS quarantine root |
| prune commands | CAS stores / docker / brew become `PruneCommand` entries; their dry-run output is captured into the manifest [03 §3][03 §5][03 §6] |
| honesty | totals = Σ `alloc_bytes` with per-(dev,ino) hardlink dedup; snapshot-pinning caveat attached [02 §3] |

```rust
pub fn build(cands: &[Candidate], pc: &PolicyConstraints, vols: &[VolumeInfo]) -> Plan;
pub fn write_manifest(p: &Plan, path: &Path) -> io::Result<()>; // fs-err; pretty JSON
pub fn load_manifest(path: &Path) -> io::Result<Plan>;
pub fn summarize(p: &Plan) -> String; // "quarantine 5.4 GiB / 11 entries; prune 1.6 GiB via 2 cmds; refuse 3"
```

Justification: the staging-manifest UX is DaisyDisk's Collector minus its permanent-unlink
ending [02 §4]; Hazel treats rules as a first-class, previewable artifact — closest cousin to
our policy→plan compiler [02 §4]; the typed byte-count gate replaces CleanMyMac's deliberately
irreversible "Remove Securely" anti-pattern [02 §2 item 6]; refusals pre-checked here map onto
the exact `errno` families `apply` would otherwise hit (`EXDEV`, `ENOTSUP`, `EDEADLK`, `EPERM`)
[05 §1][05 §2][05 §5][05 §6].

### 3.4 `execute/` — quarantine / purge / restore + WAL

Responsibility: apply, purge, restore driven by the plan; WAL writes; recovery at startup; all
operations idempotent.

Quarantine protocol ([05 §7] + [05 §1]):

| Step | Mechanism | Failure handling |
|---|---|---|
| root | per volume: `<mnt>/.Trash-janitor/` mkdir 0700 + `chflags UF_HIDDEN` (flag survives rename, empirical) | never touch Apple's `.Trashes` (root-owned ACL minefield) [05 §7] |
| op dir | `<root>/<wal-op-id>/` | restore = reverse renames recorded in WAL; purge = recursive unlink of op dir |
| pre-checks | `statfs` both ends: same `st_dev` (`EXDEV` otherwise — even across APFS volumes in one container [05 §2]); capability probe `VOL_CAP_INT_RENAME_EXCL` cached per volume (advisory — trust `errno` `EINVAL`/`ENOTSUP` at call time; the `RENAME_SWAP`-bit-mismatch precedent [05 §1 empirical]); flag pre-check `UF_IMMUTABLE`/`SF_RESTRICTED`/`SF_NOUNLINK` [05 §6]; dataless target ⇒ refuse (`EDEADLK` under the OFF policy) [05 §5] | plan already refused; execute re-verifies (TOCTOU armor) |
| move | `renamex_np(from, to, RENAME_EXCL \| RENAME_NOFOLLOW_ANY)` — moves the symlink itself; `EEXIST` ⇒ retry `-<n>` suffix; the syscall IS the existence check (no TOCTOU vs plain `rename`) [05 §1][05 §7] | `ELOOP` ⇒ a path component became a symlink; re-plan |
| post | `sync_all()` on the parent dir fd (open `O_RDONLY\|O_DIRECTORY`) | — |
| restore | reverse `renameatx_np` from the WAL's recorded `(original, quarantine)` pair; `ENOENT` benign (already restored) | volume absent ⇒ report with recorded `st_dev` + `mntonname`, not a cryptic `errno` [05 §7] |
| cross-volume | refused at plan; documented downgrade only with explicit consent: `copyfile(COPYFILE_MOVE \| COPYFILE_ALL)` non-atomic [05 §2] | — |
| WAL placement | WAL lives on the **same volume as its quarantine** (`<quarantine>/wal.jsonl`) — recovery is impossible after a cross-volume crash otherwise [08 risks #3] | — |

```rust
pub fn open(plan: &Plan, ledger: Ledger) -> Result<Executor>;
impl Executor {
  pub fn apply(&mut self, tx: Sender<ExecEvent>) -> Result<ApplyReport>;
  // per entry: wal.append(intent) -> sync_data -> renamex_np -> wal.append(result) -> parent-dir sync
  pub fn purge(&mut self, typed_bytes: u64, tx: Sender<ExecEvent>) -> Result<PurgeReport>;
  pub fn restore(&mut self, op: WalOpId, tx: Sender<ExecEvent>) -> Result<RestoreReport>;
}
pub fn recover(vols: &[VolumeInfo], ledger: &Ledger) -> RecoveryReport; // §5.2 sequence
```

Crates: libc 0.2.189 raw decls (`renamex_np`, `renameatx_np`; define the missing consts
`RENAME_NOFOLLOW_ANY=0x10`, `SF_DATALESS`, `UF_DATAVAULT` locally [05 §reusable]); nix 0.31.3
(`fstatat`+`AtFlags`, `fcntl(F_FULLFSYNC)`, `Errno` mapping); fs4 flock; fs-err for ledger
paths; `std::fs::sync_all()` already maps to `F_FULLFSYNC` on apple — no extra crate for the
data fsync [08 §2].

### 3.5 `ledger/` — JSONL history + crash recovery

Responsibility: two files in `~/.disk-janitor/` — `ledger.jsonl` (permanent, R5 台账) and
`wal.jsonl` (crash journal, checkpointed). Single writer guarded by flock. Derived state never
lives only in memory — rebuildable from the JSONL [08 §7 lesson].

Record envelope + kinds [08 §4][08 §8]:

```
{"v":1,"seq":42,"ts":"2026-08-30T12:00:00Z","kind":"apply","crc32":…,"prev_crc":…,"rec":{…}}
```

| `kind` | `rec` payload |
|---|---|
| `scan_result` | root, files_scanned, total_bytes, duration_ms, session_id |
| `verdict` | path_hash (never raw path), size, grade, evidence[] |
| `plan` | plan_id, entries[]{path_hash,size,grade,action}, policy_snapshot |
| `apply` | plan_id, entry, original_path, quarantine_path, renamex_np:true, undo{original_path,byte_count,grace_expires_at} |
| `purge` | plan_id, entry, bytes_freed (typed confirm echoed), quarantine_path |
| `restore` | plan_id, entry, from, to, undoes:<apply seq> |
| `budget` | calls, tokens, cost_usd (R6) |

```rust
pub fn open(home: &Path) -> Result<Store>; // flock sidecar LOCK_EX|LOCK_NB [08 §6]
impl Store {
  pub fn append<R: Serialize>(&mut self, kind: Kind, rec: &R) -> Result<u64>;
    // envelope + crc32fast over canonical serde_json::to_vec(rec)+seq [08 §4]
  pub fn wal_intent<W: Serialize>(&mut self, w: &W) -> Result<()>;
    // write_all + sync_data() — std maps to F_FULLFSYNC on apple [08 §2]
  pub fn sync_boundary(&mut self) -> Result<()>;
    // parent dir fd sync_all — APFS reorders two independent fullfsyncs (SQLite 3.46.1) [08 §1]
  pub fn replay(path: &Path) -> Result<(Vec<Record>, RecoveryReport)>;
    // BufRead::read_until(b'\n'); torn tail -> truncate + park *.torn;
    // mid-corrupt -> skip-and-continue + report [08 §5]
  pub fn checkpoint(&mut self) -> Result<()>;
    // replay -> temp (same dir) -> sync temp -> sync parent -> rename over [08 §1][08 §5]
}
```

Privacy asymmetry (documented decision): `verdict`/`plan` recs store `path_hash` only; the
`apply`/`restore` recs store real paths — restore needs them, and the ledger is local-only;
the LLM never reads it [08 risk #6][07 §privacy].

Crates: crc32fast 1.5.1, serde_json (canonical `to_vec` as CRC input), fs4 1.1.0 (or
`nix::fcntl::Flock`), tempfile 3.27.0 for compaction renames (we own the fsyncs — tempfile's
`persist` does NOT sync [08 §3]), fs-err. Rejected: `sled` (alpha line stalled since 2024-10;
the ledger is simple enough to own) [08 §rec].

### 3.6 `policy/` — intent → constraints

Responsibility: compile a user intent (NL sentence, GUI controls, or CLI flags) into
`PolicyConstraints`. Local rule compiler first; LLM fallback sees aggregates only.

| Input | Path |
|---|---|
| NL sentence | ordered first-match regex/keyword rules (Hazel's rule philosophy [02 §4]): "腾出 25GB" ⇒ `target_bytes`; "别动我这两周在跑的项目" ⇒ `deny_under_mta=14 d`; "别动 Xcode 归档" ⇒ `deny_globs += ~/Library/Developer/Xcode/Archives` |
| GUI controls / CLI flags | structured → `PolicyConstraints` directly |
| LLM fallback (only when local rules are uncertain) | prompt = intent sentence + `AggregateStats { total_by_grade, count_by_grade, top_dir_shapes }` — no paths; JSON-schema'd output, validated + clamped locally [07 §privacy 1,4] |

```rust
pub fn compile(src: &IntentSource, cfg: &Config) -> Result<CompiledPolicy, PolicyError>;
// CompiledPolicy { constraints, applied_rules: Vec<&'static str>, unmatched: Vec<String> }
pub fn llm_compile(sentence: &str, agg: &AggregateStats, client: &ChatClient) -> Result<PolicyConstraints>;
```

Crates: regex, serde; reuses `agent/`'s `ChatClient` (no second HTTP stack). Thresholds live in
`config/` so tuning the SPEC Part A §7 open questions (grace period, hot-mtime threshold) is a config
change, not a code change.

### 3.7 `agent/` — tool-calling loop (the privacy chokepoint)

Responsibility: an OpenAI-compatible chat-completions tool loop that decides reclaim actions;
enforces budget + stop conditions; streams a trajectory for the GUI Agent panel / Ledger screen
(SPEC Part C §3.9). The single architectural
point where the privacy invariant is enforced.

Loop shape — a sans-IO state machine (rig-run pattern; testable + serializable without a
network; matches the WAL/crash-recovery ethos [07 §rig]):

| Step | Meaning |
|---|---|
| `CallModel{turn, history}` | one `create_chat_completion` (stream:false — stream only the TUI text display [07 risks]); pre-check ctx fit with tiktoken [07 §context] |
| `CallTools{calls}` | validate the `arguments` JSON strings (repair/skip/fail hooks; `tool_retry_limit=3` hashed per name+args) [07 §wire][07 risks] |
| `Done{reason}` | `StopReason::{ModelStop, RequestedByTool, MaxTurns, TokenBudget, CostBudget, UserCancel, ToolFailLoop, ContextCap}` [07 §budget] |

Tools — no path ever crosses outward:

| Tool | Returns (redacted shape [07 §privacy 1]) |
|---|---|
| `scan_summary` | volumes + totals; starts an async scan |
| `candidates` | opaque IDs `C-001…` (deterministic size-desc order) with `{id, bytes, grade, evidence_summary[], mtime_bucket, dir_shape:"node_modules-like tree"}` |
| `build_plan` | policy constraints in ⇒ plan summary `{counts, bytes by grade, refusals}` |
| `apply` / `restore` | accept ONLY ID lists; the ID→path map is resolved inside `execute/` after the decision [07 §privacy 2] |
| `ledger_receipts` | `{id, verdict, bytes, grade}` [07 §privacy 3] |
| `record_verdict` | terminal control tool (swiftide `Stop` pattern) [07 §budget] |

Cost accounting [07 §cost]: per-call
`(prompt − cached)·p_miss + cached·p_hit + completion·p_completion` (reasoning tokens are
already inside `completion_tokens` — never double-count); provider-reported usage is ground
truth (never local tokenizers for billing); per-call usage persisted to the ledger; interrupt at
`config.budget`. Provider quirks handled: DeepSeek thinking-mode `reasoning_content` echo bloat
(avoid thinking mode in agent loops), OpenRouter final usage-chunk shape deviation (an
accounting frame, not a second turn), Ollama id-less tool calls (mint a synthetic id), lost
final usage chunk (fall back to a tiktoken estimate marked "unverified" — never billed)
[07 §providers][07 risks].

Leak discipline [07 §privacy 5]: a CI test asserts `serde_json` of every outbound message
rejects `/`, `~`, `/Users/`, `/Volumes/`, `\.(app|dmg|trash)$`; tool `description`/`name` and
JSON-Schema `description` fields carry no path or name hints (e.g. not "delete a node_modules
dir" — that leaks a name) [07 risks].

```rust
pub struct AgentRun { state: RunState, budget: BudgetState, history: Vec<Msg> }
impl AgentRun {
  pub fn next_step(&mut self, client: &ChatClient) -> Step;        // rig-run sans-IO [07 §rig]
  pub fn execute_tools(&mut self, calls: &[ToolCall], ctx: &SessionCtx) -> Vec<ToolMsg>;
  pub fn serialize_tool_result(o: &ToolOutput) -> RedactedJson;    // THE chokepoint
  pub fn snapshot(&self) -> RunSnapshot; // serializable; resume after crash/Ctrl-C
}
pub fn cost_of(u: &Usage, p: &PriceTable) -> f64;
```
Session save/load (R5 half): `RunSnapshot` persists as `~/.local/state/disk-janitor/sessions/<run_id>.json`
(`dirs::state_dir()`), atomic write via tempfile + fsync (same protocol as WAL, no rename-under-lock
hazards — sessions are per-run, no cross-process contention). Ledger gains a `Session` record kind
referencing `<run_id>` + byte offset into the run's trajectory so the ledger trajectory screen can
reload a past run; `janitor agent --resume <run_id>` (CLI) and the trajectory screen's `R` key (TUI)
call `RunSnapshot::load()` and continue `next_step()` from the restored `RunState` [R5; 08 §5].

Crates: async-openai 0.41.3 (typed types, `with_api_base` for DeepSeek/OpenRouter/Ollama/vLLM,
built-in `SimpleRetryPolicy` 429/5xx + `Retry-After`), tokio (`rt-multi-thread` confined to
this thread), tiktoken-rs 0.12 (preflight only), serde_json [07 §rec].

### 3.8 `ui/` — GUI shell (UX-SPEC) + CLI fallback

Responsibility: superseded by the GUI pivot — the product UI is now a Tauri + Svelte
desktop App specified in `docs/SPEC Part C` and ADR-18 (TUI shelved; ADR-5 amended); the
Rust core modules (§3.1–3.7, §3.9–3.10) are unchanged. The architecture contract that
survives the pivot:

- **UI thread model (§2.1)** → the Tauri main process replaces the ratatui UI thread as
  the event consumer; the scan/classify/exec/agent core threads and the
  no-async-outside-the-agent-thread rule are unchanged.
- **Channels (§2.2)** → progress streams cross the webview boundary via
  `tauri::ipc::Channel<T>` (bootstrap-then-stream, UX-SPEC Part A §2.3); crossbeam-channel
  inside the core is unchanged.
- **Thin CLI subset (ADR-14)** → `scan --json` / `plan` / `restore` / interactive purge
  remains the automation surface for CI and agent plumbing (purge never gets `--yes`, P4).

### 3.9 `config/` — `config.toml` + env fallback

Responsibility: `config.toml` (R3) + env overrides + GUI settings sheet (SPEC Part C §3.8);
secrets never logged.

```rust
pub struct Config {
  pub endpoint: Url, pub api_key: SecretString, pub model: String,
  pub ctx_len: u32, pub thinking: ThinkingMode,            // R3 fields [SPEC Part A §5]
  pub prices: Vec<ModelPrice>,                             // per 1M: prompt_miss, prompt_hit, completion [07 §cost]
  pub budget: Budget { cost_usd, max_turns, tokens },      // R6 interrupt
  pub thresholds: Thresholds { hot_mtime: 10 min, grace_hours: 72, min_candidate_bytes, ancestor_depth },
  pub ui: UiConfig { tick_ms: 100, scan_channel_bound: 100 },
}
pub fn load() -> Result<Config>; // dirs::config_dir()/disk-janitor/config.toml -> env overrides
pub fn redact(&self) -> String;  // safe debug/log form
```

Crates: serde, toml, secrecy, dirs. The DeepSeek peak/off-peak hit/miss pricing split [07 §cost]
is modeled as multiple `ModelPrice` entries keyed by model name.


### 3.10 `platform/` — per-OS abstraction layer

Responsibility: one facade per OS behind shared traits so `scan/`, `execute/`, and `ledger/`
stay OS-agnostic. Every syscall substrate difference from notes 09–11 lands here; the macOS
behavior already specified in §3.1–3.4 is the gold-standard baseline [11 §1]. Impl style copies
the proven cfg-gating idioms: trash-rs's per-OS `mod platform;`, dua-core's `mod macos; mod
windows;` + std fallback, xattr's `platforms!` macro [11 §7].

Trait sketches (fields are the contract; bodies elided):

```rust
pub struct AtomicMoveOpts { pub no_clobber: bool, pub nofollow: bool }
pub trait AtomicMove {
    /// Quarantine: no-clobber, same-volume only. `EEXIST`/`ERROR_*_ALREADY_EXISTS` family
    /// mapped to one `MoveError::TargetExists` for the `-<n>` retry loop.
    fn move_noreplace(&self, from: &Path, to: &Path, opts: AtomicMoveOpts) -> Result<(), MoveError>;
    /// Restore: replace semantics. Returns the guarantee actually delivered (§5 delta).
    fn move_replace(&self, from: &Path, to: &Path) -> Result<ReplaceGuarantee, MoveError>;
    fn probe(&self, vol: &VolumeInfo) -> AtomicitySupport; // cached per volume at scan time
}
pub enum ReplaceGuarantee { Atomic /*macOS renamex_np*/, PosixRename /*Win 1709+*/,
    MoveFileEx /*pre-1709: non-atomic w.r.t. open targets*/, CopyFallback }
pub enum AtomicitySupport { Full, FsWithGaps /*Linux NFS/Windows SMB*/, None /*refuse at plan*/ }
```

Per-OS `AtomicMove` impls [11 §1]:

| OS | no-clobber (`apply`) | replace (`restore`) | Probes / fallback |
|---|---|---|---|
| macOS | `renamex_np(from, to, RENAME_EXCL \| RENAME_NOFOLLOW_ANY)` — unchanged from §3.4 [05 §1] | `renameatx_np` reverse rename from the WAL pair | capability probe `VOL_CAP_INT_RENAME_EXCL`; `EEXIST` retry suffix [05 §1] |
| Linux | `renameat2(AT_FDCWD, src, AT_FDCWD, dst, RENAME_NOREPLACE)` — `EEXIST` on target; kernel ≥3.15, glibc wrapper ≥2.28 | plain `rename()` is always POSIX-atomic on Linux | per-volume fstype probe via `/proc/self/mountinfo` field (9): ext4 ≥3.15, btrfs/tmpfs ≥3.17, xfs ≥4.0, many ≥4.9; **NFS rejects any flags (`-EINVAL` in `nfs_rename()`)** — probe once, then `O_TMPFILE`+`linkat(AT_EMPTY_PATH)` staging or refuse auto-action [10 §0][11 §1/risks] |
| Windows | open source with `DELETE` access (`FILE_FLAG_OPEN_REPARSE_POINT \| FILE_FLAG_BACKUP_SEMANTICS`), then `SetFileInformationByHandle(FileRenameInfoEx, Flags=0)` — no-clobber = *omit* `FILE_RENAME_FLAG_REPLACE_IF_EXISTS` (0x1); POSIX_SEMANTICS (0x2) keeps replaced-file handles valid. Win10 1709+ per rust-lang#123985; [MS-FSCC] states no min version — runtime probe, never assumed | Win10 1709+: `REPLACE_IF_EXISTS \| POSIX_SEMANTICS` (atomic replace, old handles stay valid). Pre-1709: `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` — non-atomic w.r.t. open targets (`ERROR_ACCESS_DENIED`) — the exact two-tier fallback rust std uses | startup probe of `FileRenameInfoEx`; same-volume via `same-file` Handle equality (`dwVolumeSerialNumber`), not raw `st_dev`; `ERROR_NOT_SAME_DEVICE` ⇒ refuse cross-volume [09 §delta][11 §1/§4] |

**Walker.** Trait `Walker { fn walk(opts: WalkOpts) -> DirStream }` with the same `DirStat`
output contract on every OS: macOS = dua-core 3.3.0 `getattrlistbulk` module (unchanged, §3.1)
[04 §dua]; portable baseline = `ignore` 0.4.33 `WalkParallel` — the only fully-portable walker
today — with gitignore semantics disabled for honest byte accounting [11 §5]; per-OS fast paths
behind the same output, following dua-core's in-tree precedent: Linux `statx` (rustix
`StatxFlags::BTIME` for birth time; Unix `MetadataExt` `st_dev`/`st_nlink`/`st_blocks` portable
across Unix) [11 §4/§7], Windows `GetFileInformationByHandleEx(FileIdBothDirInfo)` bulk
enumeration (dua-cli measured "30x and more performance on Windows" since 2.41) [11 §5].
Windows walker rules: never descend reparse points (`IO_REPARSE_TAG_MOUNT_POINT` junctions —
cycles, other volumes, `\\?\` escapes; junction target counts as outside the root); skip
`\\wsl$` entirely (virtual 9P view; the ext4.vhdx is hands-off) [09 §0/§4]. Linux: resolve each
candidate's mount from `/proc/self/mountinfo` (field 3 major:minor = `st_dev`, field 9 fstype)
to decide `renameat2` availability [10 §0][11 §4].

**VolumeInfo.** `trait VolumeInfo { fn volumes() -> Vec<VolumeInfo>; fn capacity(&self) -> ...;
fn same_volume(&self, a: &Path, b: &Path) -> bool }`. macOS: sysinfo `Disks` + CFURL capacity
keys, `ImportantUsage` preferred (unchanged §3.1) [04 §sysinfo]. Windows:
`BY_HANDLE_FILE_INFORMATION.dwVolumeSerialNumber` / `FILE_ID_INFO` replaces `st_dev`; the
`same-file` crate's `Handle` equality (volume serial + file index, with its documented
file-index-aliasing fallback) is the portable identity check, valid on UNC paths too [09 §0][11 §4].
Linux: `statvfs`/`statfs` + mountinfo resolution; `st_dev` compare suffices including bind
mounts [10 §0][11 §4].

**DatalessCheck.** `trait DatalessCheck { fn is_dataless(&self, md: &impl MetadataLike) -> bool;
fn scan_open_policy(&self) -> OpenPolicy }` — scan must never trigger hydration on any platform:

| OS | Detection | Rule |
|---|---|---|
| macOS | `SF_DATALESS` in `st_flags`/`getattrlist`; `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` at scan start (unchanged, §2.3/§3.1) [05 §5] | read triggers download; metadata-only [05 §5] |
| Windows | `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (0x00400000) from `FindFirstFileW`/`WIN32_FIND_DATAW` or `GetFileInformationByHandleEx(FileBasicInfo)`; siblings `RECALL_ON_OPEN`, `OFFLINE`, `PINNED`, `UNPINNED`; open with `FILE_FLAG_OPEN_NO_RECALL` if a handle is unavoidable | OneDrive/cldflt placeholders (Win10 1709+): never open content, size from metadata, never include hydrated size in purge math, never move cross-volume (forces full recall) [09 §0/§3][11 §3] |
| Linux | no OS-level dataless files — fstype from `/proc/self/mountinfo` is the nearest signal (NFS/autofs/overlayfs analogues) [11 §3] | none of the iCloud/OneDrive kind; no `.placeholder` convention exists in the notes (unverified) |

Dataless evidence stays classify-side on every OS (`Dataless` ⇒ `HandsOff` / refuse move); the
xattr-based tagging alternative is rejected — xattrs are not portable (Linux per-fs, Windows
unsupported in the `xattr` crate) ⇒ the JSONL ledger is the only restore-metadata store [11 §7/risks].

Durability mapping (WAL §5.2 across platforms) [11 §2]: macOS `F_FULLFSYNC` + fsync(dirfd)
(baseline, unchanged); Linux `fsync(file)` + mandatory `fsync(dirfd)` ("calling fsync() does not
necessarily ensure that the entry in the directory … has also reached disk"), `syncfs` available
(error reporting since 5.8); Windows `FlushFileBuffers` (needs `GENERIC_WRITE`) or
`FILE_FLAG_WRITE_THROUGH` — **no directory-fsync API exists**; metadata durability rides NTFS
journaling, weaker than `F_FULLFSYNC` (documented cache→device write, no platter-barrier
request). This is the documented Windows guarantee delta of §5.

Crates (full version/license table in SPEC Part D §1f): Windows-gated `windows` 0.62.2 /
`windows-sys` 0.61.2 + `winreg` 0.56.0 behind `#[cfg(windows)]` (the crates do not compile
their Win32 modules elsewhere — keep the cfg at the module boundary) [09 §crates]; Linux-gated
`rustix` `fs::renameat_with`/`syncfs`/`statx` + `libc` `renameat2` [10 §crates][11 §rec];
portable `same-file` 1.0.6, `dirs` 6.0.0, `filetime` 0.2.29 (mtime preservation across
move/restore) [11 §rec].

## 4. Data flow — one scenario end-to-end

User request, typed in the GUI agent panel (SPEC Part C §3.9): **"腾出 25 GB，别动我这两周在跑的项目"**.

| # | Stage | What happens | Artifacts |
|---|---|---|---|
| 1 | policy | NL compiler matches local rules: `target_bytes=25 GiB`, `deny_under_mta=14 d`, `allow {PureCache, RebuildableFromSource, Redownloadable}`; no LLM call needed (rules fully cover the intent) | `CompiledPolicy` in memory |
| 2 | scan | volume info read (free_important vs raw, labeled [02 §3]); dua-core walk of `~` with materialization OFF; `DirStat` batches stream over `scan_rx`; footer = atomic snapshot. Finds: `node_modules` A 2.1 GiB, `node_modules` B 0.9 GiB, `DerivedData` 6.8 GiB, `~/.npm/_cacache` 3.2 GiB, `Docker.raw` (apparent 64 GiB / allocated 11 GiB), pnpm store (hardlinked into several projects) | `ScanTotals`; `scan_result` ledger rec |
| 3 | classify | ancestor probes: **A** — `package-lock.json` newer than dir ⇒ `RebuildableFromSource`, regen `npm ci` ≈ 40 s [03 §1]; **B** — no `package.json` in any ancestor ⇒ orphan ⇒ prompt-only [03 §0]; **DerivedData** — sibling project dirs + `IDECustomDerivedDataLocation` resolved [03 §4] ⇒ `RebuildableFromSource`; **_cacache** — hash-named children ⇒ `PureCache` CAS, whole-store action only [03 §1]; **Docker.raw** — sparse (apparent ≫ allocated) ⇒ `HandsOff` as a file + `PruneCommand docker system prune` [03 §6]; pnpm store ⇒ `nlink>1` discount; `.git/HEAD` mtimes feed `deny_under_mta` | `Candidate` stream; `verdict` recs (`path_hash` only) |
| 4 | plan | `build()`: 11 `Quarantine` entries (5.4 GiB) — A, DerivedData, `_cacache` whole-store (the mtime-hot rule is waived for whole-store actions [03 risks #7]); 2 `PruneCommand` (`docker system prune`, `brew cleanup --prune=90d` 0.4 GiB); 3 `Refuse` (node_modules B orphan ×1, `MtimeHot` ×2); Docker.raw excluded (sparse honesty: 11 GiB allocated, not 64 GiB apparent). Totals hardlink-discounted. `write_manifest` → `plan-20260830-1432.json` | manifest JSON; `plan` ledger rec |
| 5 | apply | confirm in GUI (plan-review screen, UX-SPEC Part A §3.5) → Exec thread, per entry: WAL intent `{op, entry, undo{original_path, grace_expires_at=+72 h}}` → `sync_data` (`F_FULLFSYNC` on Apple [08 §2]) → `renamex_np(RENAME_EXCL\|RENAME_NOFOLLOW_ANY)` into `<vol>/.Trash-janitor/w-0042/` → WAL result → parent-dir sync at the op boundary. **Zero bytes freed.** Ctrl-C mid-apply cancels between entries (WAL stays consistent); flock keeps a second instance out | `apply` recs; `ExecEvent`s |
| 6 | purge | review shows "5.4 GiB quarantined — purge frees 5.4 GiB"; user types `5841673728` (exact allocated); mismatch refuses. Per-entry recursive unlink; `bytes_freed` echoed into the WAL; freed report notes TM-snapshot pinning if snapshots exist [02 §3] | `purge` recs |
| 7 | restore (next morning) | a project won't build ⇒ `restore w-0042` (or the Ledger screen's one-click Restore, UX-SPEC Part A §3.7): reverse renames from the WAL `(original, quarantine)` pairs for entries not yet purged; `undoes` seq recorded; within the 72 h grace; the quarantine op dir is removed when empty | `restore` recs |
| 8 | agent overlay (same data) | user asks the agent "what's safe?": it calls `candidates` ⇒ sees only `C-001…` `{bytes, grade, evidence_summary}`; calls `build_plan`; calls `record_verdict`. It never sees a path (chokepoint + CI leak test); cost accrues per call; `Budget` interrupt fires at the cap | `budget` recs |

Byte ledger across the scenario: `apply` moves 0 bytes; `purge` frees 5.4 GiB allocated;
`restore` returns 0 net for already-purged entries (the WAL only restores surviving entries)
and fully restores any entry purged-then-regretted within the grace window.

## 5. Safety architecture

### 5.1 Quarantine protocol (invariants)

- **Same-volume by construction**: per-volume root; `statfs` re-checks both ends at apply time
  [05 §7][05 §2].
- **Hidden**: `UF_HIDDEN`, mode 0700; never Apple's `.Trashes` (root-owned ACL minefield)
  [05 §7].
- **Atomic**: `RENAME_EXCL | RENAME_NOFOLLOW_ANY`; `EEXIST` retries with a `-<n>` suffix; plain
  `rename`'s crash-atomicity guarantee plus `EXCL` flags mean no partial state [05 §1].
- **Lossless**: xattrs and BSD flags survive `renamex_np` (empirical) [05 §6].
- **Discoverable-by-us only**: the WAL holds the `(original, quarantine)` pairs; Trash Put-Back
  metadata is undocumented and is never relied on [02 risks #5].
- **Honest**: zero bytes freed at apply; `purge` requires a typed byte count equal to allocated
  bytes.

### 5.2 WAL + ledger write sequence (per operation)

1. `flock` held for the process lifetime [08 §6].
2. `wal.append(intent {op, entry, undo})`; `sync_data()` = `F_FULLFSYNC` on Apple (std does it
   since PR #60121) [08 §2].
3. Filesystem mutation (`renamex_np` / `unlinkat`).
4. `wal.append(result)`; at the op-group boundary, `sync_all()` on the parent dir fd — APFS
   reorders two independent `fullfsync`s (the SQLite 3.46.1 backport inserted exactly this
   directory sync between them) [08 §1][08 risks #2].
5. Every N records / at session end: `checkpoint` = replay valid records → temp file in the
   same dir → `sync` temp → `sync` parent → `rename` over the WAL [08 §5].

Sync granularity: per **state transition**, not per record (`F_FULLFSYNC` ≈ 4 ms vs 0.075 ms
for a plain `fsync` on Apple SSDs — amortize at the transition boundary) [08 §1][08 risk #1].

Recovery on startup [08 §5][08 §8][08 risk #5]:

1. Take `flock`; replay `ledger.jsonl` (skip-and-continue; report skipped offsets and `seq`
   gaps).
2. Replay the `wal.jsonl` tail; a record is committed iff its CRC validates.
3. Reconcile against observed FS state: `quarantine_path` exists ⇒ roll forward
   (`apply`/`restore` are re-runnable — `rename` fails benignly with `ENOENT`); otherwise drop
   the tail.
4. Verify `grace_expires_at`: expired quarantines become purge candidates (surfaced, not
   auto-purged).

### 5.3 Confirmation gates

| Gate | Mechanism |
|---|---|
| apply | plan manifest reviewed in the GUI plan-review screen (UX-SPEC Part A §3.5); apply is reversible, so the gate is soft (Enter) |
| purge | type the exact allocated byte count; mismatch refuses; this is the only irreversible step |
| prune commands | dry-run output rendered first (`brew -n`, docker filter preview); network regen is opt-in [03 §5][03 §3] |
| hard-evidence refusals | `MtimeHot`/`FileLock`/`OrphanTree`/`Dataless`/flag-blocks never enter the auto plan; surfaced as prompts [03 §0] |
| Expensive/HandsOff grades | excluded from the default policy; require explicit per-entry opt-in |
| external / exFAT volume | refuse auto-apply with evidence (no `RENAME_EXCL` / `EXDEV`) [05 §2] |
| budget | agent loop interrupted at `config.budget` (cost / tokens / turns) [07 §budget] |

### 5.4 Privacy invariant

The LLM never sees paths or names — only the intent sentence (user-authored, passed through
verbatim) and aggregate statistics [SPEC Part A §3.3][07 §privacy]. Enforcement, strongest first:

1. **Structural** — tools return opaque IDs (deterministic size-desc order); `execute` accepts
   only ID lists; the ID→path map lives in the ledger and is resolved inside `execute/` [07
   §privacy 1–2].
2. **Serializer chokepoint** — a single `serialize_tool_result`; receipts are redacted to
   `{id, verdict, bytes, grade}` [07 §privacy 3].
3. **CI leak test** — outbound `serde_json` must reject path-shaped regexes [07 §privacy 5].
4. **Tool-metadata hygiene** — names/descriptions/schema descriptions are path-free [07 risks].

Asymmetry, documented: the ledger stores real paths for undo (local-only; the LLM never reads
it) [08 risk #6].

### 5.5 Platform guarantee delta (Windows)

The §5.1 quarantine invariants hold as written on macOS. On Windows the OS substrate is
weaker and the delta is documented, not hidden [09 §delta][11 §1/§2]:

- **apply (no-clobber)** remains atomic and TOCTOU-free — `SetFileInformationByHandle(
  FileRenameInfoEx, Flags=0)` fails if the target exists, on Win10 ≥1607. This is the
  RENAME_EXCL analog [09 §delta][11 §1].
- **restore (replace)** is fully atomic (old handles stay valid) only on Win10 ≥1709 with
  `REPLACE_IF_EXISTS | POSIX_SEMANTICS`; on 1607–1708 it degrades to `MoveFileExW(
  MOVEFILE_REPLACE_EXISTING)`, which is non-atomic with respect to open targets
  (`ERROR_ACCESS_DENIED`/sharing violations). Restore correctness still holds because the
  WAL is written first; only concurrent-reader races surface as errors [11 §1/risks].
  [MS-FSCC] documents no minimum OS version — the 1709 gate is enforced by a runtime probe
  with `MoveFileExW` fallback, never assumed [09 risks #1][11 §1].
- **Crash durability**: no directory-fsync API exists on Windows and `FlushFileBuffers` is
  weaker than `F_FULLFSYNC` (no platter-barrier request); NTFS journaling carries metadata
  durability. **Consequently, restore-on-Windows relies on the WAL journal — the recorded
  `(original_path, quarantine_path)` pairs — rather than any OS Trash semantics.** The
  shell Recycle Bin (`IFileOperation`) is at most a purge-time handoff surface, never the
  restore path [11 §2/§6].
- Same guarantee-shaping rules as macOS apply: cross-volume moves are refused at plan;
  network shares (SMB has no POSIX rename semantics) are treated as external volumes [11 risks #2].

### 5.6 Interrupt semantics + uninstall + format versioning

**Interrupt semantics (D-15).** A scan/classify interrupt drops incomplete batches —
completed batches are kept, and the UI says exactly that, never a fake "scan complete"
(P6). An interrupted `apply` is WAL-auto-resumed at the next launch (§5.2 recovery rolls
forward committed intents). Once confirmed, `purge` runs to completion: its entries are
independent, so there is no mid-purge stop — interruption is not offered at that gate.

**Uninstall contract (ADR-16, D-17).** Uninstall requires restore-or-explicit-confirm of
every surviving quarantine across all volumes before any state removal — 恢复是权利 holds
to the end; orphaned per-volume quarantine dirs (no matching WAL) are surfaced for manual
resolution, never silently deleted.

**Data-format versioning (ADR-17, D-18).** Ledger / WAL / manifest format v1 is frozen
through the course; replay is forward-compatible — unknown record kinds and unknown
fields are skipped **and counted** (`skipped_unknown=N`), `deny_unknown_fields` stays
OFF, and post-course schema changes only via version-gated, logged migration passes.

## 6. Key type definitions

Rust-ish pseudocode. Field lists are the contract; method bodies elided.

```rust
// ---- scan / classify ----
pub struct Candidate {
    pub id: Cid,                 // "C-042": deterministic size-desc order [07 §privacy 1]
    pub path: PathBuf,           // local only — NEVER serialized outward
    pub path_hash: [u8; 8],      // ledger + agent-facing identity
    pub dev: u64, pub ino: u64,  // hardlink dedup key
    pub alloc_bytes: u64,        // honest reclaimable number; used for all displayed totals
    pub app_bytes: u64,          // apparent size, informational only
    pub grade: Grade,
    pub evidence: Vec<Evidence>, // >=1 Strong signal required [03 §0]
    pub regen: Option<Regen>,    // command + measured cost
    pub mtime: SystemTime, pub nlink: u32, pub clone_id: Option<u64>,
    pub locked: bool,            // flock marker / open-handle probe
}
pub enum Grade { PureCache, RebuildableFromSource, Redownloadable, Expensive, HandsOff }
pub struct Evidence { pub kind: EvidenceKind, pub detail: String, pub strength: Strength }
pub enum EvidenceKind {
    LockfilePresent, SiblingManifestMissing, OrphanTree, MtimeHot, FileLock,
    GitRepoAncestor, CasStore, Hardlinked, ApfsClone, Sparse, Dataless,
    FlagBlock(UfSf), QuarantineXattr, Unreadable,
}
pub enum Strength { Weak, Medium, Strong }
pub struct Regen { pub command: String, pub cost_secs: Option<u32>, pub network: bool }

// ---- plan ----
pub struct PlanEntry {
    pub cid: Cid, pub path: PathBuf, pub path_hash: [u8; 8],
    pub alloc_bytes: u64, pub grade: Grade,
    pub action: PlanAction,
    pub evidence: Vec<Evidence>,    // carried into the manifest for review [02 §4]
}
pub enum PlanAction {
    Quarantine,
    PruneCommand { cmd: String, dry_run: String },
    Refuse { reason: Refusal },
}
pub enum Refusal {
    MtimeHot, FileLock, OrphanTree, Dataless, ImmutableFlag, CrossVolume,
    NoRenameExcl, ReadOnly, BelowMinBytes, GradeNotAllowed,
}

// ---- ledger / WAL ----
pub struct Record<R> {
    pub v: u8, pub seq: u64, pub ts: DateTime<Utc>, pub kind: Kind,
    pub crc32: u32, pub prev_crc: Option<u32>, pub rec: R,
}
pub enum LedgerRecord {                 // rec payloads per kind [08 §8]
    Scan(ScanRec), Verdict(VerdictRec), Plan(PlanRec), Apply(ApplyRec),
    Purge(PurgeRec), Restore(RestoreRec), Budget(BudgetRec),
}
pub struct ApplyRec {
    pub plan_id: PlanId, pub entry: PlanEntryHash,
    pub original_path: PathBuf,          // real path: undo requires it [08 risk #6]
    pub quarantine_path: PathBuf, pub byte_count: u64,
    pub grace_expires_at: DateTime<Utc>, // now + 72 h
}
pub struct WalEntry {                    // intent record — written + synced BEFORE the mutation
    pub op_id: WalOpId, pub kind: WalKind,  // Intent | Result
    pub plan_id: PlanId, pub entry: PlanEntryHash,
    pub from: Option<PathBuf>, pub to: Option<PathBuf>,
    pub bytes: u64, pub undo_of: Option<u64>, // seq of the apply this restore undoes
}

// ---- policy ----
pub struct PolicyConstraints {
    pub target_bytes: Option<u64>, pub min_bytes: u64,
    pub allow_grades: Vec<Grade>,
    pub deny_globs: Vec<Pattern>,        // user paths (local only)
    pub deny_under_mta: Duration,        // "don't touch what I ran this fortnight"
    pub allow_network_regen: bool, pub allow_prune_commands: bool,
    pub grace_hours: u64, pub max_entries: u32,
}
```

Invariants worth testing:

- **ID stability** — `Candidate.id` is a pure function of the scan result: sort candidates by
  `alloc_bytes` desc, tie-break by `path_hash`; identical scans yield identical IDs (so a
  resumed agent run re-addresses the same candidates).
- **Byte honesty** — `alloc_bytes` is used wherever a byte count is shown or typed; `app_bytes`
  is informational. Hardlinked/clone bytes are attributed once per `(dev, ino)` / `clone_id`.
- **Replay self-sufficiency** — every `LedgerRecord`/`WalEntry` is self-contained for replay;
  the only cross-record reference is `undo_of` (a `seq`), so a single corrupted line cannot
  poison replay of independent later operations [08 §5].

## 7. Testing strategy

| Layer | Approach |
|---|---|
| Fixture trees | `tests/fixtures/gen.py` (or a Rust bin) builds synthetic trees: node_modules+lockfile, orphan `.venv`, DerivedData-shaped dir, `_cacache` CAS store, pnpm store with hardlinks (`fs::hard_link`), a sparse file (seek+write), a symlink chain + loop, a `UF_HIDDEN` quarantine root (chflags via libc). Dataless cannot be synthesized in userspace (`SF_DATALESS` is kernel-synthetic, read-only [05 §6]) — unit tests mock the flag on `Metadata`-like values (classifier takes the flag as input, not raw lstat); one `#[ignore]`'d integration test runs only where a real iCloud dataless file exists (opt-in, materialization-OFF assertions). Golden classify outputs; property: Σ child `alloc` == parent `alloc` with hardlinks discounted once per `(dev, ino)`. |
| Syscall-level | `#[cfg(target_os = "macos")]` integration tests on real APFS (`/tmp`): `RENAME_EXCL`→`EEXIST`; `EXDEV` probe vs `/System/Volumes/VM` (skip if absent); `F_FULLFSYNC` rc=0 via nix; `UF_HIDDEN` survives `renamex_np`; hardlink `nlink` preserved across rename; `RENAME_NOFOLLOW_ANY`→`ELOOP` on a symlink path component; `clonefile`→`EEXIST`; capability-probe mask asserted advisory-only [05 §1]. |
| Platform matrix | CI matrix `macos-latest` / `windows-latest` / `ubuntu-latest` on every push: macOS runs the macOS `Syscall-level` suite above on APFS; Linux runs `renameat2(RENAME_NOREPLACE)`→`EEXIST` on ext4/tmpfs, `EXDEV` across mounts, fsync(file)+fsync(dirfd) WAL ordering, `/proc/self/mountinfo` fstype resolution, NFS/skip gating; Windows runs `FileRenameInfoEx(Flags=0)`→target-exists error, `MoveFileExW` fallback path, `dwVolumeSerialNumber` same-volume check, junction non-descent, `\\?\` long-path manifest round-trip, OneDrive `RECALL_ON_DATA_ACCESS` detection (skip if OneDrive absent) [09 §0/§delta][10 §0][11 §1/§4]. Syscall tests are `#[cfg]`-gated per OS (`#[cfg(target_os = "macos")]`, `#[cfg(target_os = "linux")]`, `#[cfg(windows)]`); portable layers (ledger/WAL, plan, classify fixtures, TUI snapshots, agent loop) run on all three runners. |
| Crash injection (WAL) | spawn a child `janitor` applying a plan with `KILL_AT ∈ {before_wal_sync, after_wal_sync_before_rename, after_rename_before_result, mid_batch}`; `kill -9`; parent runs `recover()` and asserts: committed ops present exactly once, no phantom ops, torn tail truncated + parked to `*.torn`, `seq` gaps reported. Optional deep probe via `dtrace`/`FSInOut` for cross-file ordering [08 risks #2]. |
| Ledger property tests | random op sequences + random truncation points → replay invariants hold (skip-and-continue, monotonic `seq` after gaps, no CRC false positives) [08 §5]. |
| GUI (purge modal + flows) | Svelte component tests + Tauri `ipc::Channel` mock: the typed byte-count modal rejects a mismatched byte string; interrupt semantics per §5 (apply auto-resume, purge runs to completion); evidence panel renders a full `Evidence` chain; bilingual labels CN/EN per ADR-13. Wireframe conformance per SPEC Part C §3. |
| (shelved TUI row kept for traceability) | ratatui `TestBackend` snapshot tests per screen [06 §rec] — superseded by the GUI row above (ADR-5/18). |
| Scanner edge cases | firmlink walk of `/` (no double-walk of the Data side) [04 §4]; an iCloud-shaped tree scanned with materialization OFF (no downloads — assert no content reads via unchanged mtime/size); a mounted volume appearing mid-tree (`st_dev` reject in `descend`). |
| Agent | sans-IO `RunState` serde round-trip + resume; a stub OpenAI-compatible HTTP server (e.g. wiremock) covering: malformed `arguments` JSON (repair path), DeepSeek `reasoning_content` echo shape, OpenRouter final usage-chunk deviation, Ollama id-less tool calls, missing final usage chunk (estimate marked unverified, not billed) [07 §providers, risks]; leak-test regexes on every outbound message [07 §privacy 5]. |
| NFR (D-13/D-14) | criterion-style bench: ≥1 GiB/s metadata-only scan on M1 Pro, home dir <2 min — CI-gated on the macOS runner; memory-ceiling fixture: 1M synthetic entries assert RSS < 500 MB. |

## 8. Build sequencing

Course deadlines: 9/1 comments, 9/6 public demo, 9/8 tryouts, 9/10 presentation [SPEC Part A §6].

| Milestone | Scope | Concrete work items | Exit criterion |
|---|---|---|---|
| **M1** scan + classify core | `scan/`, `classify/` + fixtures | cargo workspace skeleton; dua-core integration (`walk_roots`, `Order::ParentFirst`, `descend` dev-check); `setiopolicy_np` `extern "C"` + constants [04 risks]; `VolumeInfo` via sysinfo; fixture generator; classify path rules v1 (node/rust/python/xcode [03 §1–4]) + evidence probes (lockfile, orphan, mtime, nlink); `janitor scan --json` CLI | scanning this Mac yields classified JSON with honest `alloc` bytes |
**Platform sequencing** (deadlines first, platforms second): M1 stays macOS-first for the
9/1 comment deadline — the `scan/`/`classify/` core, fixtures, and the dua-core macOS walker
land on this Mac; the `platform/` traits (§3.10) are sketched at M1 so M2 can slot impls in
without re-plumbing, but only the macOS impl is exercised until M2 [09][10][11].

| **M2** plan / apply / purge / restore + WAL + ledger | `plan/`, `execute/`, `ledger/` | `renamex_np` layer + capability probe; per-volume quarantine + per-volume WAL; record framing (crc32fast envelope); flock; `recover()`; plan builder + manifest write/load; CLI `plan/apply/purge/restore`; crash-injection harness (`KILL_AT`). **Platform: M2 wires the trait impls for Linux** — `AtomicMove` via `renameat2(RENAME_NOREPLACE)` + per-volume fstype probe (`/proc/self/mountinfo`), fsync(file)+fsync(dirfd) durability pair, XDG cache roots, `requires-elevation` refusal class [10 §0/delta][11 §1] | CLI round trip incl. `kill -9` recovery passes |
| **M3** GUI shell | `ui/` | GUI shell per SPEC Part C (Tauri+Svelte) [12]: Tauri 2.11.5 shell + Svelte 5.57 app; named commands + `ipc::Channel` progress streams per §3.8; Dashboard/Review/Plan/Apply screens per UX-SPEC Part A §3; purge gate modal; settings sheet v0. **Platform: M3 keeps Windows on the portable baseline** — `ignore::WalkParallel` walker, std path resolution; no Windows-specific UI work [11 §5] | full flow usable in the GUI (9/6 demo) |
| **M4** agent + NL policy + token/cost | `agent/`, `policy/`, `config/` | `config.toml` full (R3); NL policy compiler v1; sans-IO `AgentRun` + 6 tools + chokepoint + leak test; usage/cost accounting + budget interrupt; trajectory screen. **Platform: M4 keeps Windows on the portable baseline** (same as M3); Windows `AtomicMove`/`VolumeInfo` impls (`FileRenameInfoEx`, `dwVolumeSerialNumber`, junction rules) land behind `#[cfg(windows)]` only when CI can exercise them [09 §delta][11 §1/§4] | "腾出 25GB" runs end-to-end under budget |
| **M5** polish | docs, demo, hardening | demo = product video (Screen Studio/Hyperframes) + live preview (D-21/D-22); AI-log per-milestone export to `docs/ai-logs/M<N>-<date>.md` (D-19); 开销明细表 with standard columns (date / model / calls / prompt+completion tokens / cost USD+CNY) generated from ledger `budget` recs, budget-rec `model` field per D-20; README; SPEC.md final sync; threshold tuning (SPEC Part A §7); edge-case hardening list from §7 | 9/10 presentation |

Dependency notes:

- The §5.2 WAL protocol (reconcile WAL records vs observed FS state) must be spec'd *before*
  coding M2 — it is the open question [08 risk #5].
- Platform: the CI matrix of §7 (macos/windows/ubuntu) starts at M2 — before that, Windows
  and Linux builds are compile-checked locally, not behavior-tested; Windows syscall paths
  (`FileRenameInfoEx` probes, junction handling) are untestable until a windows runner exists,
  which is why M3/M4 keep Windows on the portable baseline [09 risks #1][11 §5].
- dua-core is ~10 days old as a standalone crate (first release Aug 2026, ~10k downloads,
  v3.3.0) — pin the exact version, audit `crates/dua-lib/src/macos` unsafe (`attrlist` buffers)
  before shipping; the pre-planned fallback is `ignore::WalkParallel` [04 risks].
- M3's purge modal depends on M2's byte-count contract; M4's agent depends on M1+M2+M3 tool
  surfaces being stable.

## 9. Design references — what each module borrows

| Module | Project | Borrowed | License (verified) | Note |
|---|---|---|---|---|
| scan | Byron/dua-cli → dua-core 3.3.0 | whole walker: work-stealing pool, drop-to-cancel, `getattrlistbulk` macOS module, `descend` device check, `Throttle` redraw gating | MIT [04 §rec] | jwalk is dead — README redirects to dua-core [04] |
| scan | dundee/gdu | ticker→atomic-snapshot→draw progress loop; cancel-keeps-partial-results; OSC 9;4 terminal progress; dataless prevention | MIT [04] | |
| scan | GuillaumeGomez/sysinfo | volume enumeration + `ImportantUsage` preference | MIT [04] | |
| classify | tbillington/kondo | known-project-structure discovery per language | MIT [03 §rec] | direct precedent for path rules |
| classify | holmgr/cargo-sweep | toolchain-keyed `target/` pruning rules; `--stamp` workflow | MIT (repo) [03 §2] | unmaintained — borrow rules only |
| classify | matthiaskrgr/cargo-cache | `~/.cargo` component size accounting | MIT/Apache [03 §rec] | |
| classify | zaldih/npkill | node_modules listing UX | MIT [03 §rec] | |
| plan | DaisyDisk (behavior ref) | Collector staging UX; refuse-list for system dirs | closed [02 §4] | staging kept, permanent-unlink replaced by quarantine |
| plan / policy | Noodlesoft Hazel (behavior ref) | rules as a first-class previewable artifact; Revert Files validates post-hoc undo demand | closed [02 §4] | |
| execute | SQLite | WAL frame validity + recovery algorithm; dir-fsync discipline; journal commit styles | public domain [08 §7] | |
| execute | Stebalien/tempfile | `persist_noclobber` atomic-rename pattern (we add the fsyncs) | MIT/Apache [08 §3] | |
| execute | ArturKovacs/trash | per-volume macOS trash semantics — reference only, NOT used for moves | MIT [05 §rec] | |
| ledger | git (design ref only) | content addressing + trailer-checksum ideas | GPL-2.0 — read-only [08 §7] | do not import code |
| agent | 0xPlaygrounds/rig | sans-IO `AgentRun` state machine; `Tool`/`ToolDefinition`; `max_turns` budget | MIT [07 §rig] | |
| agent | bosun-ai/swiftide | `StopReason` enum + control tools (`Stop`→`record_verdict`); `tool_retry_limit` hashed per call | MIT [07 §rig] | |
| agent | 64bit/async-openai + openai/openai-node | typed chat-completions; `finish_reason` union; `include_usage` semantics | MIT / Apache-2.0 [07] | |
| ui | Byron/dua-cli TUI | event-loop skeleton; name-keyed selection restore; `ctrl+c` keymap | MIT [06 §rec] | |
| ui | gitui-org/gitui | multi-source `select!` loop; `any_work_pending()` gating | MIT [06 §rec] | |
| ui | gdu / yorhel/ncdu | SIGINT→synthetic key; rate-limited redraw; abort-and-keep-results | MIT [06 §rec] | |
| config | dua/gdu config patterns; async-openai `OpenAIConfig` | config shape; `with_api_base` | MIT [07] | |
| cross-cutting | alienator88/Pearcleaner | orphan-search heuristics + Sentinel Trash monitoring to read | Apache-2.0 + Commons Clause — do NOT vendor [02 §rec] | |
| cross-cutting | tw93/Mole; grandperspectiv | dry-run UX ideas; scan-result persistence | GPL-3.0 / GPL-2.0 — ideas only [03 §rec][02 §rec] | |

### Reusable-code license posture

| Repo | License | Posture |
|---|---|---|
| dua-cli / dua-core, kondo, cargo-sweep, cargo-cache, npkill, gdu, ncdu, gitui, ratatui, crossterm, async-openai, rig, swiftide, tempfile, crc32fast, fs4, nix, libc, rustix, sysinfo, serde*, walkdir, ignore/globset, rayon, crossbeam, signal-hook, unicode-width, indicatif, tiktoken-rs, fs-err, same-file | MIT / Apache-2.0 / Unlicense / PD | vendoring & borrowing safe — verify each per-file at adoption |
| Pearcleaner | Apache-2.0 + Commons Clause v1.0 | read for behavior; do not vendor into a product [02 §rec] |
| GrandPerspective, git, tw93/Mole | GPL-2.0 / GPL-3.0 | read as design reference only — never import code [02 §rec][03 §rec] |
| apple/darwin-xnu | APSL-2.0 | reference for syscall semantics; do not paste code [05 §reusable] |

---

*End of Part B (formerly DESIGN.md) — iterate in lockstep with the code (course
rule). Open questions from SPEC Part A §7 (grace period, threshold tuning, APFS snapshot interactions,
external-volume purge semantics — now a per-OS matrix, see §3.10/§5.5) are addressed in
§5/§7/§8 and remain config-tunable. Platform amendment 2026-08-30: notes 09–11; macOS
baseline preserved, Linux/Windows deltas documented in §3.10 and §5.5.*


# Part C — UX design

> Status: **accepted 2026-08-31** (owner-confirmed GUI pivot, decision register D-09–D-12).
> This Part C **replaces the old DESIGN.md §3.8** (ratatui TUI) as the UI authority; the Rust
> core (scan/classify/plan/execute/ledger/agent/policy/config, SPEC Part B §3.1–3.7, §3.9–3.10)
> is unchanged. Stack: Tauri 2.11.5 + wry 0.56.1 + Svelte 5.57 (all Apache-2.0 OR MIT,
> verified 2026-08-31) [12]. Citations: decision register `D-NN`; research notes
> [12] = `docs/research/12-tauri-gui-stack.md`, [13] = `docs/research/13-disk-tool-product-presentation.md`;
> core behavior cites Part B §/ADR. Companion doc: design-decisions.md (ADR-18).

## 1. Product identity

**Name.** 「还我磁盘」 — Chinese primary, English secondary everywhere in the UI (D-02,
D-12). The name is the promise: 还我 = "give it back" — 删除是操作，恢复是权利
(deletion is an operation, restoration is a right) [SPEC Part B §1].
**Brand (D-23, owner 2026-09-01).** "Disk Janitor" is retained as the internal/working title
only — a live name-collision audit found 4+ shipping products using it (note 20 §1). The
product brand is **还我磁盘** (CN, primary); the EN identity is a **coinage on the undo
promise** — the 归还/restore gesture, not a generic "cleaner" noun — pending a final
candidate pick (note 20 §3, direction B).

**Positioning.** An evidence-driven, reversibility-first disk cleaner with an Apple-native
feel, usable by people who barely know what it is (D-10). It answers the question `du`
can't: not "where are the bytes?" but "which of these are safe to reclaim, why, and how do
I get them back if I'm wrong?" [SPEC Part A §1]. Three pillars drive every screen:

| Pillar | What it means | Where the UI proves it |
|---|---|---|
| 证据驱动 Evidence-driven | every verdict carries inspectable machine-derived evidence, never name matching [SPEC Part B §1] | Evidence panel on every candidate (§3.4); refusal rows show the timestamp/lock as proof [13 §I] |
| 可恢复优先 Reversibility-first | `apply` frees 0 bytes; `purge` is the only irreversible step, gated by a typed byte count [SPEC Part B §5.1] | Ledger + Restore are first-class (§3.7, §4.3); 72 h grace countdown [ADR-10] |
| Apple 原生质感 Apple-native feel | frictionless, native chrome/menus/dialogs [12] | Titlebar, vibrancy, native dialogs per §5.5; system dialogs for dangerous ops (§7) |

**10-second pitch.**

- CN: 「看清哪些能删、为什么能删——删错了，一键拿回来。扫描只读元数据，文件不出你的 Mac。」
- EN: "See what's safe to reclaim and why — and take it back with one click if you're
  wrong. Scans read metadata only; your files never leave the Mac."

**Tone: calm, trustworthy — the anti-fear cleaner.** Competitor research [13 §B, §H]
shows the incumbent playbook (threat counters, "Neutralize threats before they do any
harm") reliably earns community distrust; every trusted 2026 entrant instead leads with
privacy + recoverability [13 §E]. Disk Janitor's voice rules:

| Say ✓ | Never say ✗ | Source |
|---|---|---|
| Specific honest numbers: 「19.3 GB 可回收 · 0 文件被销毁，直到你确认」 "19.3 GB recoverable · 0 files destroyed until you confirm" | Threat counters, "329K threats removed", fear of invisible harm | [13 §G5][13 §H] |
| 「Developer ID 签名并公证」 "Developer ID signed + notarized" | "Notarized by Apple" as endorsement shorthand | [13 §H][D-16] |
| 「我们清理字节」 "We clean bytes." | RAM boost / performance-miracle claims | [13 §H] |
| 「这一步不可撤销」 "This step cannot be undone." — stated plainly at the purge gate | Hiding the safety model; countdown-to-permanent-unlink endings (DaisyDisk Collector) | [13 §C] |
| 「零遥测 · 无账号」 "Zero telemetry · no account" stated on screen | Silent telemetry; undisclosed network calls | [13 §D][D-06] |

Refusals are voiced, never hidden: every refusal = verdict + evidence + safe alternative
(P1, D-03) — showing a refusal on screen is more persuasive than any badge [13 §I].

## 2. Information architecture

### 2.1 Window structure

| Surface | Decision | Rationale |
|---|---|---|
| Main window (label `main`) | **one** main window: left sidebar + content + collapsible right agent dock | a single focused flow; no window juggling for non-technical users [D-10] |
| Settings | **sheet inside the main window** (⌘,), not a separate window | one addressable surface; R3 config fields are few [SPEC Part B §3.9] |
| Purge confirm | **in-window modal sheet**; outside-click and Esc are inert (§4.2) | the signature moment must not be escapable by accident [P4] |
| Native dialogs | folder pickers + destructive-op confirms via `tauri-plugin-dialog` (sheet-style on macOS) [12] | dangerous ops use system dialogs, not HTML imitations [12] |
| Tray icon | **not in v1** — nothing runs in background by design (no daemon, SPEC Part A §8) | a tray with no real behavior is noise; revisit only with a real background feature |
| About / privacy | panel inside Settings incl. the zero-telemetry statement [D-06] | privacy stated on screen [13 §G6] |

Window chrome per the Apple-native recipe [12]: `titleBarStyle: "Transparent"` +
`hiddenTitle: true` (real traffic lights, content extends under the titlebar);
vibrancy via `window-vibrancy` 0.8.0 with `transparent: true` + `macOSPrivateApi: true`
(fine for direct-DMG distribution; only forecloses the App Store route) [12]; geometry
persisted by the window-state plugin [12]. A second launch focuses the existing window
(single-instance plugin [12]; the core's `flock` already refuses a second writer,
SPEC Part B §3.5).

### 2.2 Screens

| Screen | Purpose | Entry |
|---|---|---|
| 仪表盘 Dashboard | volume truth, last-scan summary, one primary action | default on launch; first-run state per D-05 |
| 扫描 Scan view | volume/folder selection, live progress ring, cancel | Dashboard action |
| 审核 Review — **the heart** | evidence inspection; choose what enters the plan | after scan; Dashboard summary |
| 计划 Plan review | the manifest: quarantine / prune / refusals → apply | Review → 生成计划 |
| 活动 Activity | live apply/purge/restore progress + WAL-resume notices | auto during ops; from Ledger |
| 台账 Ledger | audit trail; restore affordance within grace | sidebar |
| 助手 Agent panel | propose-only chat: plan cards + evidence; human confirms [D-01] | global right dock (⌘J), any screen |
| 设置 Settings | scan/policy defaults, agent config (R3), language, appearance, uninstall | sheet |

Navigation model: the sidebar holds 4 persistent items (Dashboard / Review / Ledger /
Settings); Scan view, Plan review, and Activity are contextual stages; the Agent panel is
a global right dock. Esc always backs out of a *soft* stage (scan setup, plan review) and
never out of the purge gate (§4.2). Multi-window notices (e.g. "purge finished") use
`emit_to` broadcast events, not channels [12].

### 2.3 Data flow into the UI

Bootstrap-then-stream [12]: on mount, a snapshot command seeds state (`get_volumes`,
`get_ledger_tail`, `get_plan`), then a typed `tauri::ipc::Channel<T>` delivers ordered
deltas — never polling [12]. All scan/classify/plan/apply/purge I/O goes through ~10
named Rust commands; the webview never receives raw fs verbs, and the Tauri capability
file stays deny-by-default [12].

## 3. Screen-by-screen wireframes

Conventions for every wireframe below:

- Chrome is shared: transparent titlebar with real traffic lights, vibrancy backdrop,
  4-item sidebar, collapsible right agent dock (⌘J) [12] (§2.1–2.2). The sidebar is
  drawn in full once (§3.2) and abbreviated `(sidebar)` elsewhere.
- **Bilingual label contract (D-02, D-12, ADR-13):** every control renders 中文 primary +
  English secondary — 「开始扫描 Start scan」. Dense rows may drop the EN tail when
  width-bound; a screen never mixes CN-only and EN-only full labels. i18n keys carry
  both strings; composition is the component's job (§6, ADR-13).
- Wireframes pin content, hierarchy, and states — not pixels. Component mapping: §6.

### 3.1 Grade color language

Risk-tier color on every candidate row, plan entry, and ledger record is table stakes
[13 §I] (DevCleaner's SAFE/WARNING/DANGER chips [13 §A]); ours maps the five rebuild-cost
grades [ADR-8][SPEC Part B §6]. Every color pairs with a glyph + word (colorblind-safe,
theme-following per §5.5):

| Grade | Badge (bilingual) | Color | Plan policy |
|---|---|---|---|
| PureCache | ● 纯缓存 Pure cache | green | default allow |
| RebuildableFromSource | ● 可重建 Rebuildable | teal | default allow; regen cost shown |
| Redownloadable | ● 可重下载 Redownloadable | blue | default allow; network cost stated |
| Expensive | ⚠ 代价高 Expensive | amber | per-entry opt-in only [SPEC Part B §5.3] |
| HandsOff | 🔒 请勿动手 Hands off | red | never auto-plan; size shown, nothing more |

Refusals reuse the glyphs: ⚠/🔒 + refusal reason + evidence — never a bare strikethrough
[13 §I]. Color means exactly one thing on a row — "how wrong could this go" — bytes and
paths stay neutral.

### 3.2 仪表盘 Dashboard

```
┌──────────────────────────────────────────────────────────────────────┐
│ 仪表盘 Dashboard                                        ⌘J 助手 Agent │
├──────────────┬───────────────────────────────────────────────────────┤
│ 仪表盘       │  Macintosh HD · APFS · 可写 writable                  │
│ Dashboard    │                                                       │
│ 审核 Review  │  已用 Used               612.4 GB / 1 TB              │
│ 台账 Ledger  │  ███████████████████████████░░░░░░░░░░░               │
│ 设置         │  真实可用 Free (raw)      387.6 GB                    │
│ Settings     │  快照锁定 Snapshot-held  13.6 GB ¹                    │
│              │                                                       │
│ ──────────   │  上次扫描 Last scan    今天 14:32 · 1,204,553 files   │
│ (⌘J) 助手    │  可回收 Recoverable    19.3 GB · 214 项 candidates    │
│ Agent        │  已销毁 Destroyed      0 — 直到你确认 until confirm  │
│              │                                                       │
│              │  ┌──────────────────────────────┐                     │
│              │  │  开始扫描 Start scan          │  ← 唯一主操作 CTA   │
│              │  └──────────────────────────────┘                     │
│              │  [ 一键清理 One-tap clean ] 二级操作 (D-24)             │
│              │                                                       │
│              │  ¹ Time Machine 本地快照锁定的空间，过期后释放         │
│              │    freed after snapshot expiry [SPEC Part B §3.1]          │
└──────────────┴───────────────────────────────────────────────────────┘
```

Entry: default at launch; the scan button is the only primary CTA (D-05 — dashboard + hint
line, no wizard state machine). A secondary **一键清理 One-tap clean** (D-24) becomes
enabled only after a scan exists; it auto-marks grades **PureCache + RebuildableFromSource**
(owner 2026-09-01, note 15 §2/§6) and surfaces an explicit confirm list 「N 项待确认 N
awaiting confirm」 for anything else — it never implies it saw everything.
Volume truth shows raw free and snapshot-held space as two
labeled numbers [SPEC Part B §3.1]. 「0 已销毁」 is the anti-fear hero line [13 §G5].

Empty state (first run, D-05):

```
│           还没有扫描记录 No scans yet.                              │
│   扫描只读元数据，文件不会离开你的 Mac。                             │
│   Scans read metadata only — nothing leaves this Mac.               │
│           [ 开始第一次扫描 Start first scan ]                       │
```

Error state: unreadable volume → refusal banner in P1 format (§5.2) — 判定 verdict +
errno evidence + alternative (reselect root / replug / scan another volume).

### 3.3 扫描 Scan view

```
┌──────────────────────────────────────────────────────────────────────┐
│ 扫描 Scan · ~/ (Macintosh HD)                     [更改目标 Change]   │
├──────────────┬───────────────────────────────────────────────────────┤
│ (sidebar)    │                     ╭───────────╮                     │
│              │                   ╱               ╲                   │
│              │                  │     63 %        │                  │
│              │                  │  812,443 项     │  1.21 GiB/s [D-13]│
│              │                   ╲  当前 current ╱                  │
│              │                     ╰───────────╯  ~/Library/Develop… │
│              │                                                       │
│              │  候选 Candidates  214 · 19.1 GB    根错误 Root errs 2 │
│              │                                                       │
│              │  [ 取消 Cancel ]   中断 = 丢弃未完成批次，保留已完成   │
│              │                    interrupt drops incomplete [D-15] │
└──────────────┴───────────────────────────────────────────────────────┘
```

- Progress ring fed by a typed `Channel<ScanProgress>` (≥100 ms throttle) — never
  polling [12] (§5.3).
- Cancel per D-15: incomplete batches are dropped, completed classifications kept; the
  banner states exactly that — no fake "scan complete" (P6).
- Root errors stream inline as refusal rows; the scan continues (skip-and-continue
  posture, [ADR-4]).

### 3.4 审核 Review — the heart

Left = virtualized candidate table; right = evidence panel of the selected row. Every
row carries its grade badge (§3.1) and allocated bytes; every verdict is inspectable —
this panel is where 证据驱动 Evidence-driven stops being a slogan [13 §I].

```
┌──────────────────────────────────────────────────────────────────────┐
│ 审核 Review · 214 候选 candidates · 19.3 GB   [筛选 Filter ▾][排序 Sort ▾] │
├──────────────┬───────────────────────────────┬───────────────────────┤
│ (sidebar)    │ ✓  ~/code/api/node_modules    │ 证据链 Evidence chain │
│              │   ● 可重建 Rebuildable        │ ① 锁文件 lockfile:    │
│              │     2.1 GB   npm ci ≈ 40 s    │    package-lock.json  │
│              │                               │    newer than dir     │
│              │ ✓  ~/Library/Developer/…      │ ② 祖先 .git 活跃      │
│              │   DerivedData                 │    HEAD 3 h ago       │
│              │   ● 可重建 Rebuildable        │ ③ mtime 冷却 cool     │
│              │     6.8 GB   rebuild ≈ 90 s   │    6 d 14 h           │
│              │                               │ 重建 Rebuild:         │
│              │ ✓  ~/.npm/_cacache            │ `npm ci` ≈ 40 s       │
│              │   ● 纯缓存 Pure cache (CAS)   │ 网络 network: 不需要  │
│              │     3.2 GB                    │                       │
│              │ ✕  ~/code/api/node_modules/…  │                       │
│              │   ⚠ 正在写入 actively written │ 判定 REFUSED 拒绝     │
│              │     mtime 4 min ago          │ [03 §0][SPEC Part B §5.3] │
│              │                               │                       │
│              │ [ 214 项中 197 已选 197/214 ]  │                       │
│              │                               │                       │
│              │ [ 生成计划 Build plan ]       │ [在访达中显示 Reveal] │
└──────────────┴───────────────────────────────┴───────────────────────┘
```

| Rule | Spec |
|---|---|
| Selection | checkbox per row; refusals (✕) render **disabled**, not hidden — the refusal is the trust demo [13 §G3][13 §I]; reason row is tappable → evidence panel shows the exact signal (mtime 4 min ago) |
| Evidence panel | the candidate's `evidence: Vec<Evidence>` chain [SPEC Part B §6], numbered ①②③, each line = kind + human detail + strength; regen command + measured cost + network flag rendered verbatim [ADR-8] |
| Grades Expensive / HandsOff | ⚠/🔒 rows are listed but locked: 「显示大小，绝不自行处理」 shows-you-size-never-touches posture [13 §A DevCleaner]; opt-in is an explicit per-row toggle with a confirm chip [SPEC Part B §5.3] |
| Byte honesty | allocated bytes only (hardlink/clone-discounted); apparent size on hover/详情 [SPEC Part B §6 invariants] |
| Virtualization | windowed list, O(visible) rebuilds; selection re-found by stable path key when live data reorders rows [SPEC Part B §3.8][12] |
| Sort/filter | grade, size, mtime, path — filters never re-hide refusals (they pin trust) |

Empty state (scan done, nothing found):

```
│        扫描完成 — 没有值得回收的东西。Scan complete — nothing worth     │
│        reclaiming. 这本身就是好消息。That is good news.               │
```

Error state: evidence probe fails (permission denied) → row shows ⚠ 证据不可读 evidence
unreadable + errno, row stays ⚠-refused (never silent, never auto-included) (P1, P6).

### 3.5 计划 Plan review

The manifest is the artifact [02 §4 Hazel][SPEC Part B §3.3]: terraform-plan style, refusals
listed beside actions, totals hardlink-discounted, snapshot caveat attached.

```
│ 计划 Plan · plan-20260831-1432.json          [查看 JSON View manifest] │
│                                                                       │
│  操作 Actions                                                         │
│   11 × 隔离 Quarantine    5.4 GiB   （进入隔离区，0 字节释放          │
│                                        0 bytes freed [SPEC Part B §5.1]） │
│    2 × 执行清理命令 Prune    1.6 GiB   docker system prune / brew     │
│                                        cleanup --prune=90d            │
│   3 × 拒绝 Refused        —       ⚠ 孤儿树 orphan ×1 · ⚠ mtime ×2    │
│                                                                       │
│  合计 Total (去重 dedup)         19.3 GB · 72 h 宽限 grace [ADR-10]  │
│  快照注意 Snapshot note: 13.6 GB 在快照过期后才真正释放               │
│           freed only after TM snapshot expiry [SPEC Part B §3.1]           │
│                                                                       │
│  [应用 — 隔离 Apply — quarantine]   软确认 soft gate：可直接 Enter    │
└───────────────────────────────────────────────────────────────────────┘
```

Apply gate is soft (reversible): plan reviewed → Enter → quarantine moves begin (§4.3).
Purge is NOT offered here — it lives behind the applied-quarantine state (§4.2), so the
irreversible step can never be one click from a review list [02 §4 GrandPerspective].

### 3.6 活动 Activity (apply / purge / restore progress)

Contextual stage auto-entered during operations; every row streams over the exec channel,
strictly sequential, WAL-ordered [SPEC Part B §3.4].

```
┌──────────────────────────────────────────────────────────────────────┐
│ 活动 Activity · 隔离中 quarantining · plan-20260831-1432              │
├──────────────┬───────────────────────────────────────────────────────┤
│ (sidebar)    │ ✓ node_modules (api)      2.1 GB   隔离 quarantined   │
│              │ ✓ DerivedData             6.8 GB   隔离 quarantined   │
│              │ ⟳ _cacache                3.2 GB   移动中 moving…     │
│              │ ○ docker system prune     —       等待 pending       │
│              │                                                       │
│              │ 3/13 · 已移动 moved 12.1 GB · 释放 freed 0 字节 bytes │
│              │                                                       │
│              │ [ 取消 Cancel ]  中断安全：WAL 已记录，重启自动续跑   │
│              │                  WAL-recorded, auto-resumes [D-15]   │
└──────────────┴───────────────────────────────────────────────────────┘
```

- 「释放 freed 0 字节」 runs live on every apply — the reversibility claim, made
  numeric on screen [13 §G2].
- Interrupt (D-15): apply = cancel between entries, WAL auto-resumes next launch; a
  resumed launch shows a WAL-resume notice row first (「已恢复中断的操作 resumed
  interrupted operation」) — never a silent catch-up (P6).

### 3.7 台账 Ledger

Append-only audit trail, user-inspectable, never silently deleted (P5) — rendered from
ledger records, not a shadow state [SPEC Part B §3.5].

```
┌──────────────────────────────────────────────────────────────────────┐
│ 台账 Ledger · 1,204 条 records · [筛选 kind ▾] [导出 Export JSONL]   │
├──────────────┬───────────────────────────────────────────────────────┤
│ (sidebar)    │ #842  restore  ✓ 已恢复 restored                      │
│              │       DerivedData → ~/Library/Developer/…             │
│              │       undoes #815 · 宽限 grace 余 69 h                │
│              │ #815  apply    ✓ 隔离 quarantined  2.1 GB             │
│              │ #812  plan     plan-20260831-1432 (11 隔离 · 3 拒绝)  │
│              │ #801  purge    🔒 已销毁 destroyed 5.4 GiB            │
│              │       字节数已确认 byte count confirmed: 5841673728   │
│              │ #777  budget   deepseek-chat · 12 calls · $0.0031     │
│              │                                                       │
│              │ [ 恢复… Restore… ]  ← 选中 apply 记录时可用           │
└──────────────┴───────────────────────────────────────────────────────┘
```

- Restore affordance lives here within the grace window: select an `apply` record →
  恢复 Restore (§4.4). The ledger hosts a **恢复站 Recover vault tab** (D-25, owner
  2026-09-01, note 17 §6): the actionable subset of the ledger — only `apply` records inside
  72 h grace, grouped by plan session, per-row size + countdown (剩 Xh Ym), 待销毁 pending
  section for expired (never auto-purged, ADR-10), and a banner surfacing WAL-replay resume
  after an interrupted apply (D-15). Countdown clock is ledger-recorded monotonic elapsed,
  never wall clock (D-29, note 17 risks #4). Expired-grace survivors surface as purge
  candidates with the
  countdown shown, never auto-purged [ADR-10].
- `purge` rows keep the typed byte count visible forever — the audit is the proof
  (P5, P7). Every public claim is generated from this ledger, never hand-typed (P7).
- Export = raw JSONL copy (same bytes the product replays); no telemetry channel
  exists to send it anywhere (D-06).

> **Error catalog (D-28).** The E-#### failure/error table (E1001–E3003, CN/EN copy in P1
> shape) is the paste-ready appendix to this Part C — source of truth:
> `docs/research/18-failure-error-catalog.md`.

### 3.8 设置 Settings (sheet, ⌘,)

```
┌──────────────────────────────────────────────────────────────────────┐
│ 设置 Settings                                                   ⌘,   │
│                                                                       │
│  扫描 Scan                                                            │
│    默认根 roots            [~/ ] [更改 Change…] (native picker [12])  │
│    忽略 glob 忽略 globs    [node_modules 除外除外 …]                  │
│    阈值 thresholds         mtime 热区 10 min · 最小 50 MB  [高级 Adv] │
│                            (thresholds live in config.toml [SPEC Part B §3.9]) │
│                                                                       │
│  助手 Agent (R3)                                                      │
│    endpoint   [https://api.deepseek.com/v1 ]   模型 model [deepseek-chat ] │
│    API key    [•••••••••••• ]  (secrecy 存储; 永不写入日志 [SPEC Part B §3.9]) │
│    预算 budget [ $2.00 ] [ 200k tok ] [ 40 turns ]  到达即中断        │
│                    interrupt at cap [ADR-9][SPEC Part B §3.7]              │
│                                                                       │
│  语言 Language          ( ) 中文 primary  ( ) English primary         │
│                          switch applies live; menus re-label [12]     │
│  外观 Appearance        ( ) 跟随系统 Follow system  ( ) 浅 Light      │
│                          ( ) 深 Dark                                  │
│                                                                       │
│  隐私 Privacy                                                          │
│    ✓ 零遥测 Zero telemetry — 无账号 no account · 网络仅限 LLM 请求   │
│      network is LLM calls only (D-06)[13 §G6]                         │
│                                                                       │
│  卸载 Uninstall…          ← restore-or-confirm contract (ADR-16)      │
│         [取消 Cancel]                                  [保存 Save]    │
└───────────────────────────────────────────────────────────────────────┘
```

Uninstall row (D-17, ADR-16): opens the restore-or-confirm dialog — list every
surviving quarantine with sizes; user restores or explicitly confirms each; only when
the list is empty does the app remove `~/.disk-janitor/` state. The brand invariant
恢复是权利 is enforced at uninstall time, not just in marketing copy.

### 3.9 助手 Agent panel (right dock, ⌘J)

Propose-only by construction (D-01, P2): the agent returns plan IDs and verdict cards;
every card's action button routes through the same human confirm as any other row — the
LLM never executes quarantine moves [ADR-7][SPEC Part B §3.7].

```
┌───────────────────────────────┐
│ 助手 Agent            [折叠 ×] │
├───────────────────────────────┤
│ 你 You: 腾出 25 GB，别动我这两 │
│ 周在跑的项目                   │
│                               │
│ 助手 Agent:                   │
│ ┌─ 计划卡片 plan card ───────┐│
│ │ plan-20260831-1501         ││
│ │ 11 隔离 · 2 prune · 3 拒绝 ││
│ │ 19.3 GB (去重 dedup)       ││
│ │ 预览见计划页 see Plan page ││
│ │ [在计划中打开 Open in Plan]││ ← routes to §3.5; no
│ └───────────────────────────┘│   exec button exists
│ 拒绝 refused: 2 项 mtime 热  │
│ 证据 evidence: 正在写入…      │
│ 替代 alternative: 稍后重扫    │
│                               │
│ 本轮 turn: 3 · tokens 4,102   │
│ 成本 cost $0.0009 [ADR-9]    │
└───────────────────────────────┘
```

- Cards cite evidence IDs, never paths — the agent-side payload is opaque-ID only;
  the chokepoint guarantees it [ADR-7 §structural]. 「在计划中打开」 is the only CTA:
  approval happens in Plan review, on the human's screen.
- Cost footer per turn; budget interrupt surfaces as a distinct stop-reason banner
  (「已到预算上限 budget reached」) with the count of un-run turns [ADR-9].
- Trajectory history reloads from session snapshots (`RunSnapshot`), matching the
  ledger's reproducibility posture (P7) [SPEC Part B §3.7].

## 4. Key flows

### 4.1 First scan + TCC pre-flight

The first-run state is a dashboard + hint line, not a wizard (D-05). But `~/Library`,
`~/.npm`, external volumes may trip macOS TCC (privacy prompts); the app pre-flights
readability so the user never discovers a half-scanned tree after the fact.

```
[开始扫描 Start scan]
   │
   ├─ native folder picker (tauri-plugin-dialog, sheet-style) [12]
   │
   ├─ pre-flight probe: metadata-only getattrlist on each root's top level
   │     ├─ all readable → scan starts (ring, §3.3)
   │     └─ EPERM on root(s) → TCC pre-flight banner, scan does NOT half-run:
   │
│    ┌──────────────────────────────────────────────────────────────┐
│    │ ⚠ macOS 需要授权 macOS needs permission for ~/Library        │
│    │ 证据 evidence: probe returned EPERM (系统设置系统设置▸        │
│    │   隐私与安全性▸文件与文件夹 Privacy & Security ▸ Files)      │
│    │ 替代 alternative: [打开系统设置 Open Settings] (deep link)   │
│    │   或从该根中排除 or exclude this root and continue           │
│    │   [排除并继续 Exclude & continue] [重试 Retry]               │
│    └──────────────────────────────────────────────────────────────┘
```

Refusal voice = P1 exactly: verdict (cannot read) + evidence (errno + the exact
Settings pane) + safe alternative (deep link or exclude-and-continue) (D-03). The
banner is generated from the probe result, never from a guess (P7).

### 4.2 Review → plan → apply → purge — the typed byte-count modal (signature moment)

The purge gate is the product's signature moment [13 §G2]: the one irreversible step,
made deliberate — a single final typed total for one-tap flows, never per-batch
(D-27, note 15 risks #2). No other tool gates on the user typing the exact byte count
[ADR-10][SPEC Part B §5.3].

```
(after apply completes: 「已隔离 quarantined 5.4 GiB · 释放 freed 0 字节」)
   │
   ├─ 台账 Ledger / Dashboard: [清理隔离区 Purge… ] on the op
   │
   ├─ modal sheet (in-window, §2.1 — outside-click and Esc are INERT [P4])
   │
│    ┌────────────────────────────────────────────────────────────────┐
│    │        清理隔离区 Purge quarantine — 这一步不可撤销             │
│    │                    This step cannot be undone. [13 §C]         │
│    │                                                                │
│    │  将永久销毁 permanently destroy:                                │
│    │   11 entries · 5,841,673,728 bytes allocated (5.4 GiB)         │
│    │   = 同一个数字 the same hardlink-discounted number the plan     │
│    │     displayed [ADR-10 honesty check]                           │
│    │                                                                │
│    │  其余 72 h 内可恢复 everything else restorable within grace    │
│    │                                                                │
│    │  请输入字节数以确认 type the byte count to confirm:             │
│    │  [ 5,841,673_         ]   ← 精确匹配 required，否则拒绝        │
│    │                                                                │
│    │  [取消 Cancel]              [销毁 Destroy] (disabled until      │
│    │                                          exact match)          │
│    └────────────────────────────────────────────────────────────────┘
   │
   ├─ mismatch → shake + 「字节数不匹配 count mismatch — 已拒绝 refused」
   │    (typed attempt never logged anywhere — it is not ledger data)
   ├─ exact match → purge runs to completion (D-15: no mid-purge stop,
   │    progress reads-only); typed count echoed into the `purge` ledger
   │    record as bytes_freed [SPEC Part B §3.5]
   └─ done: 「已释放 freed 5.4 GiB · 已销毁 destroyed 11 entries」
        + TM-snapshot pinning caveat if snapshots exist [SPEC Part B §3.1]
```

Design rules of the moment:

- The number typed is **allocated** bytes, hardlink/clone-discounted — computed from
  the same totals the plan showed, so a mismatch means the plan changed under the user
  [ADR-10].
- Esc / outside-click inert (§2.1): the gate must not be escapable by accident — but
  equally must not trap: Cancel works, always; only the *confirm* is deliberate.
- 销毁 Destroy renders as red + 🔒-adjacent styling (grade language §3.1), and the
  modal states the plain truth 「这一步不可撤销」 [13 §C] — no countdown-to-unlink
  theater (DaisyDisk Collector anti-pattern [02 §2]).
- Purge never reachable from Review/Plan directly (§3.5): only from an *applied*
  quarantine state, so every pre-purge step is reversible by construction.

### 4.3 Apply (quarantine) semantics

Soft gate → sequential WAL-ordered moves → live Activity (§3.6). Interrupt = cancel
between entries; WAL auto-resumes next launch (D-15). Zero bytes freed throughout —
the Activity header keeps 「释放 freed 0 字节」 on screen for the whole operation
[13 §G2][SPEC Part B §5.1].

### 4.4 Restore — the undo moment

The moment no competitor shows on camera [13 §G1] — and the brand's proof: 删除是操作，
恢复是权利 [SPEC Part B §1]. Entry: Ledger `apply` record → [恢复 Restore] (§3.7), or the
resumption notice after an interrupted apply.

```
│ [恢复 Restore] → confirm sheet:                                       │
│                                                                       │
│   恢复 DerivedData (6.8 GB) → ~/Library/Developer/Xcode/…             │
│   reverse renames from the WAL pair · undoes #815 [SPEC Part B §3.4]       │
│   宽限 grace: 剩 remaining 69 h of 72 h [ADR-10]                      │
│                                                                       │
│   [取消 Cancel]                          [恢复 Restore] (soft gate)   │
│                                                                       │
│ → done banner: 「已恢复 restored · 回到原位 back where it was」        │
│   ledger `restore` rec with undoes:<apply seq> [SPEC Part B §3.5]          │
```

- Restore is a *soft* gate — it is the safe direction; friction belongs to destruction,
  not to recovery (P4's corollary).
- After restore, the original path is re-verified. Collision triad (D-26, default = rename):
  重命名 rename (default — reverse-rename to `original-<n>`, nothing overwritten) / 跳过 skip
  (record `restored: skipped`) / 覆盖 replace (typed-byte confirm, never silent, P6).
  Evidence: Time Machine Keep-Both grammar, note 17 §5 / note 02 §4.
- Purged entries: restore is impossible and the Ledger says so plainly (「已销毁，无法
 恢复 destroyed — not restorable」) — honesty over false comfort (P6). The 72 h grace
  countdown is always visible on restorable rows [ADR-10].

## 5. Interaction rules

### 5.1 Propose-only agent (D-01, P2)

| Rule | Spec |
|---|---|
| Output shape | plan ID + verdict cards + refusal explanations (§3.9). The agent's only mutation of UI state is *suggesting*; plan approval, apply, purge, restore confirmations are human-only actions on Rust-side commands |
| No exec path | the agent dock renders no destructive button; 「在计划中打开 Open in Plan」 is its only CTA. Enforcement is architectural (tools accept/return IDs; execution lives in `execute/`), not a hidden button [ADR-7][SPEC Part B §3.7] |
| Privacy | payloads opaque-ID only; the serializer chokepoint + CI leak test guarantee no path ever reaches the model [ADR-7] |
| Budget | cost/token/turn counters visible per turn (§3.9 footer); interrupt = distinct banner naming the stop reason [ADR-9] |
| Failure | unreachable endpoint / malformed tool output → the dock states it (「助手不可用 agent unreachable — 本地流程不受影响 local flow unaffected」) and the product remains fully usable offline (P3, P6) |

### 5.2 Refusal format — 拒绝 + 证据 + 替代 (D-03, P1)

Every refusal — classifier, policy, volume, TCC, agent — renders in one fixed shape:

> **判定 verdict** — what cannot happen and to what.
> **证据 evidence** — the machine-derived fact (errno, timestamp, lock, missing
> manifest), tappable to the evidence panel.
> **替代 alternative** — the safe path forward (exclude root, run the regen command,
> wait for mtime to cool, open Settings).

The three parts are never omitted, never reordered. Rationale: showing a refusal with
its evidence is more persuasive than any trust badge [13 §I], and unvoiced refusals are
exactly the "black-box judgment" the product exists to replace [02 §4].

| Refusal source | Example render |
|---|---|
| MtimeHot | ⚠ 拒绝自动处理 refuse auto-action · 证据: mtime 4 min ago（正在写入） · 替代: 稍后重扫 rescan later |
| OrphanTree | ⚠ 项目已删除？ project deleted? · 证据: no package.json in any ancestor · 替代: 手动确认后单独处理 review manually |
| TCC / EPERM | ⚠ 无法读取 cannot read ~/Library · 证据: probe EPERM · 替代: [打开系统设置] / 排除该根 |
| exFAT / cross-volume | ⚠ 不支持原子隔离 atomic quarantine unsupported · 证据: `VOL_CAP_INT_RENAME_EXCL` missing / EXDEV · 替代: 只读分析 analyze read-only |
| Budget | 已到预算上限 budget reached · 证据: $2.00 / 200k tok / 40 turns 中已触发 · 替代: 提高上限或继续本地流程 raise cap or continue locally |

### 5.3 Progress — `ipc::Channel`, never polling (D-15 substrate, [12])

- Scan/classify: typed `Channel<ScanProgress>` batches, ≥100 ms throttle, ordered
  delivery — the channel *is* the state feed; bootstrap snapshot command seeds, channel
  takes over (§2.3) [12].
- Apply/purge/restore: exec events stream over the same channel pattern; rows are
  WAL-ordered because the executor is [SPEC Part B §3.4].
- Events (`emit_to`) only for low-frequency cross-window notices — never progress
  (out-of-order risk documented in [12]) [12].
- Cancel: a task handle in managed state (`tauri::State`) owns cancellation; the UI
  cancel button flips it, the core decides safe stop points per D-15.

### 5.4 Interrupt semantics (D-15)

| Operation | Interrupt = | UI behavior |
|---|---|---|
| scan / classify | drop incomplete batches | cancel banner; completed classifications kept; never reports "complete" (P6) |
| apply | cancel between entries; WAL auto-resume next launch | Activity shows 「已取消，重启后继续 cancelled, resumes next launch」 |
| purge | **none** — runs to completion once confirmed | progress becomes read-only; Cancel absent (the modal warned; D-15) |
| restore | cancel between entries; WAL re-runnable | same as apply |

The purge row is the point: a confirmed irreversible op does not negotiate. Every other
operation is resumable because the WAL already owns its state [SPEC Part B §5.2].

### 5.5 Theme + appearance

- Default: follow system (`windows[].theme` omitted; `prefers-color-scheme` CSS drives
  the webview; `ThemeChanged` forwarded only for menu/tray chrome sync) [12].
- In-app override (§3.8): 浅 Light / 深 Dark / 跟随系统 Follow system; override writes
  `localStorage` + sets the window theme explicitly [12].
- Grade colors (§3.1) are CSS variables per theme — meaning never rides on hue alone
  (glyph + word always present).
- Vibrancy backdrop (§2.1) on macOS; Windows Mica/Acrylic; Linux none (compositor
  rules) — degrade loudly where unsupported, never fake the effect (P6) [12].

## 6. Component inventory

Svelte 5 (runes) components; stores are `$state` class instances fed only by Channel
`onmessage` handlers [12]. Every text-bearing component takes bilingual label pairs
(`{zh, en}`), composition per §3 conventions (D-02, D-12; ADR-13).

| Component | Purpose | Notes |
|---|---|---|
| `AppShell.svelte` | sidebar + content + agent dock host | window chrome, keyboard map (§2.1–2.2) |
| `Sidebar.svelte` | 4 persistent nav items + agent toggle | bilingually labeled, active state |
| `GradeBadge.svelte` | ●/⚠/🔒 grade chip (§3.1) | CSS-variable colors, glyph+word always |
| `BilingualLabel.svelte` | 「中文 English」 pair | dense mode drops EN tail (§3 conventions) |
| `VolumeCard.svelte` | used/free/snapshot-held bars (§3.2) | two labeled free numbers [SPEC Part B §3.1] |
| `ScanProgressRing.svelte` | ring + rate + current root (§3.3) | Channel-fed; no polling |
| `CandidateTable.svelte` | virtualized rows w/ checkboxes (§3.4) | O(visible); path-key selection restore [SPEC Part B §3.8] |
| `EvidencePanel.svelte` | numbered evidence chain + regen line (§3.4) | renders `Vec<Evidence>` verbatim [ADR-8] |
| `RefusalRow.svelte` | ✕ row in P1 format (§5.2) | disabled, never hidden [13 §I] |
| `PlanManifest.svelte` | actions/refusals/totals/snapshot note (§3.5) | mirrors manifest JSON; 查看JSON opens raw |
| `ActivityList.svelte` | per-entry exec rows + freed-0 header (§3.6) | WAL-resume notice row support |
| `LedgerView.svelte` | record stream + restore affordance (§3.7) | raw JSONL export |
| `PurgeGateModal.svelte` | typed byte-count sheet (§4.2) | Esc/outside inert; match gate [ADR-10] |
| `RestoreSheet.svelte` | restore confirm + grace countdown (§4.4) | soft gate |
| `AgentDock.svelte` | chat + plan cards + cost footer (§3.9) | propose-only CTAs [D-01] |
| `SettingsSheet.svelte` | scan/agent/language/appearance/uninstall (§3.8) | native dialog for roots picker [12] |
| `UninstallDialog.svelte` | restore-or-confirm quarantine list (§3.8) | ADR-16 contract |
| `TccBanner.svelte` | permission refusal banner (§4.1) | P1 shape; deep link |
| `stores/scanState.ts` etc. | `$state` stores per concern | updated only in `onmessage` [12] |
| `i18n/{zh-CN,en}.json` | string tables | `svelte-i18n`, zh-CN primary, en fallback [12] |

State discipline: stores never poll; bootstrap-then-stream (§2.3) is the only data path;
every component's listeners torn down on destroy (channel-lifecycle leak warning [12]).

## 7. Competitor-learned patterns (note 13)

What the product takes from [13]'s survey — and what it deliberately refuses:

| From | Pattern | Our use |
|---|---|---|
| DevCleaner [13 §A] | risk-tier chips (SAFE/WARNING/DANGER) | grade color language (§3.1); DANGER-tier never pre-selected posture [13 §H] |
| DevCleaner [13 §A] | specific hero number ("23.7 GB") | Dashboard's 「19.3 GB 可回收」+「0 已销毁」 (§3.2) [13 §G5] |
| DaisyDisk [13 §C] | beauty + speed honesty; metadata-only privacy promise | scan ring with rate (§3.3); 「文件不出你的 Mac」 everywhere (§3.2, §3.8) |
| DaisyDisk [13 §C] | **rejected:** Collector ends in 5 s countdown → permanent unlink | typed byte-count gate instead (§4.2) [ADR-10] |
| Hazel [13 §C via 02] | rules/manifest as first-class artifact | plan manifest page (§3.5) |
| Stats [13 §D] | telemetry disclosure builds trust | zero-telemetry stated on screen (§3.8) [D-06] |
| Drezze [13 §E] | "moves to Trash so it's recoverable" as trust metaphor | superseded: quarantine + WAL restore, stronger than Trash (§4.4) |
| CleanMyMac [13 §B] | **rejected:** threat counters, fear framing, "Notarized by Apple" endorsement shorthand | anti-fear voice table (§1); "Developer ID 签名并公证" wording only [D-16] |
| — [13 §F] | Screen Studio-style demo conventions | 90 s structure with restore as climax (§4.4); demo planning deferred (X-02, D-21) |

Open lane noted by [13]: no major competitor ships real Simplified-Chinese product copy —
CN-primary bilingual UI (ADR-13) is a differentiator, not just accessibility [13 §delta].


# Part D — Dependencies (crate plan)

Verified 2026-08-30 (core/lanes) and 2026-08-31 (GUI layer §1g, INFERENCE batch in §3).
Every version/license claim cites a research note:
[04] `docs/research/04-rust-walk-and-metadata-crates.md` · [05] `05-macos-fs-edge-cases.md` ·
[06] `06-tui-stack-ratatui.md` · [07] `07-llm-agent-loop-rust.md` · [08]
`08-ledger-wal-crash-safety.md` · [09] `09-windows-junk-taxonomy.md` · [10]
`10-linux-junk-taxonomy.md` · [11] `11-cross-platform-primitives.md` ·
[12] `12-tauri-gui-stack.md` (GUI layer, §1g) · [13]
`13-disk-tool-product-presentation.md` (product-presentation patterns; cited in §1g notes only).
[01] (`01-oss-disk-analyzers.md`) and [02] (`02-macos-cleanup-landscape.md`) are cited only for
supplementary facts (dust config precedent; Commons Clause license caveat; serde licensing).
[03] (`03-dev-junk-taxonomy.md`) grounds rule-data provenance in §1 commentary.
Owning modules (SPEC layout): scan / classify / plan / execute / ledger / policy / agent / ui / config
/ platform (§1f — per-OS gated dependencies from notes 09–11) / gui (§1g — Tauri + Svelte, ADR-5/18).

## 1. Crates table

### 1a. Scan — walking + metadata

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| dua-core | 3.3.0 (2026-08-28) [04] | MIT [04] | PRIMARY parallel walker: crossbeam work-stealing pool (no rayon), macOS `getattrlistbulk` bulk metadata, streaming iterator, drop-to-cancel, `descend` predicate per-entry `st_dev` mount skip [04] | — | scan | ignore 0.4.33 `WalkParallel` [04] |
| ignore | 0.4.33 (2026-08-04) [04] | Unlicense OR MIT [04] | gitignore/glob exclusion (`gitignore::GitignoreBuilder` usable standalone) + fallback parallel walker; only maintained walker with built-in gitignore + `same_file_system` [04] | — | scan, classify, policy | globset raw [04] |
| globset | 0.4.20 (2026-08-04) [04] | Unlicense OR MIT [04] | fast glob-set matching for user ignore/policy globs; shares engine with `ignore` [04] | — | classify, policy | — |
| walkdir | 2.5.0 [04] | Unlicense/MIT [04] | serial ordered traversal base; fallback for sorted/serial scans; stable (no release since 2024-03) [04][05] | — | scan | fs + recursion [05] |
| jwalk | 0.9.0 (2026-08-05) [04] | MIT [04] | REJECTED — README: "Unmaintained … use dua-core"; no device check in source, no gitignore, `skip_hidden` on by default [04] | — | — (none) | dua-core [04] |
| crossbeam-channel | 0.5.16 [04] | MIT OR Apache-2.0 [04] | scan→ui event channel + `select!` loop; the dua/gdu progress backbone [04][06] | — | scan, ui | std mpsc (no select) [06]; tokio mpsc (drags tokio in for no benefit) [06] |
| num_cpus | 1.17.0 [04] | MIT OR Apache-2.0 [04] | worker count for dua-core `threads`; dua itself depends on it; `std::thread::available_parallelism` covers most needs [04] | — | scan | std [04] |
| sysinfo | 0.39.6 [04] | MIT [04] | volume list, capacity, removable/read-only flags (`Disks::new_with_refreshed_list`, `DiskRefreshKind` to trim cost) [04] | — | scan, ui | libc `statfs` manual [04] |
| rayon | 1.12.0 [04] | MIT OR Apache-2.0 [04] | parallel map for CPU-bound classify only; `par_bridge()` is a documented bottleneck for IO iterators [04] | — | classify | std threads + channel [04] |
| dashmap | 6.2.1 [04] | MIT [04] | ONLY if classifier threads share maps; avoid by keeping scan→classify channel-based (MSRV 1.70) [04] | — | classify | `RwLock<HashMap>` [04] |

### 1b. macOS syscall / execute / ledger stack

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| libc | 0.2.189 [04][05] (0.2.187 stable line [08]) | MIT OR Apache-2.0 [05][08] | raw FFI: `renamex_np`/`renameatx_np`, `clonefile`, `getattrlist(bulk)`, xattr family, `F_FULLFSYNC=51`, `F_BARRIERFSYNC=85`, `RENAME_EXCL=4`; MISSING consts to define locally: `SF_DATALESS`, `UF_DATAVAULT`, `SF_FIRMLINK`, `RENAME_NOFOLLOW_ANY`, `RENAME_RESOLVE_BENEATH`, `RENAME_SECLUDE`, `IOPOL_*` [05]; `setiopolicy_np` NOT exported — own `extern "C"` block [04] | — | execute, scan, ledger | hand-written `extern "C"` blocks [05] |
| nix | 0.31.3 [05][08] | MIT [05][08] | typed errno + `fstatat`+`AtFlags`, `fcntl(F_FULLFSYNC)`/`F_BARRIERFSYNC`/`F_GETPATH(_NOFIRMLINK)` (apple targets), `FileFlag` bitflags (lacks `SF_DATALESS`/`UF_DATAVAULT`/`SF_FIRMLINK`), `Flock` RAII; `RenameFlags` is Linux-only; NO `renamex_np` [05][08] | `fs` needed for fcntl enums [05] | execute, ledger | rustix [05] |
| rustix | 1.1.4 [05] | Apache-2.0 OR MIT OR Apache-2.0-WITH-LLVM-exception [05] | safe fd/path APIs + full `fs::xattr` module; `Arg` path trait avoids UB on interior NULs; no macOS rename-with-flags [05] | — | execute | xattr crate [05] |
| xattr | 1.6.1 [05] | MIT OR Apache-2.0 [05] | minimal cross-platform xattr get/set/list/remove; fine if rustix feels heavy [05] | — | execute | rustix `fs::xattr` [05] |
| tempfile | 3.27.0 (2026-03-11) [08] | MIT OR Apache-2.0 [08] | atomic temp file + rename for WAL compaction/snapshot replace; `persist_noclobber` atomic on Apple via `renameat_with(NOREPLACE)` since 3.18.0; docs: does NOT fsync for you [08] | — | ledger, execute | hand-rolled libc `renamex_np` [08] |
| crc32fast | 1.5.1 (2026-08-22) [08] | MIT OR Apache-2.0 [08] | per-record CRC32 framing of JSONL ledger/WAL records; SIMD + streaming `Hasher` [08] | — | ledger | `crc` crate (more variants, slower) [08] |
| fs4 | 1.1.0 [08] | MIT OR Apache-2.0 [08] | advisory `flock` wrapper (rustix-based); preferred over fs2; sync feature MSRV 1.75 [08] | sync MSRV 1.75 [08] | ledger | fs2 0.4.3 [08]; `nix::fcntl::Flock` [08] |
| fs2 | 0.4.3 [08] | MIT OR Apache-2.0 [08] | advisory lock reference / unmaintained-ish predecessor; keep as documented fallback [08] | — | ledger | fs4 [08] |
| fs-err | 3.3.1 (2026-07-03) [04][08] | MIT OR Apache-2.0 [04][08] | drop-in `std::fs` with op + path in error messages — for ledger/WAL/manifest IO, NOT the hot walk path [04][08] | optional `tokio`/`path_facts` features [04] | ledger, execute, config | manual `map_err` [04] |
| trash | 5.2.6 [05] | MIT [05] | NOT for quarantine moves (Finder-Trash semantics, not our WAL); optional interop to show where Apple trash lives [05] | — | execute (optional) | none (skip) [05] |

### 1c. TUI stack (feature `tui`)

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives | Notes |
|---|---|---|---|---|---|---|---|
| ratatui | 0.30.2 (2026-06-19) [06] | MIT [06] | widget/diffed-render library; umbrella crate re-exporting ratatui-core 0.1.2 + ratatui-widgets 0.3.2 (workspace-split since 0.30) [06] | `crossterm` default backend [06] | ui | cursive (ncurses; heavier) [06] | ~~shelved (TUI→GUI pivot, ADR-5/18)~~ |
| crossterm | 0.29.0 [06] | MIT [06] | raw mode, alt screen, events, SIGWINCH→Resize; MUST pin "0.29" — ratatui 0.30 hard-depends on it (non-optional); two majors in one tree = lost events / broken raw-mode restore [06] | `event-stream` opt-in (not needed; threads+channels) [06] | ui | termion 4.0.6 (MIT, Unix-only; no poll/timeout, no Resize events) [06] | ~~shelved (TUI→GUI pivot, ADR-5/18)~~ |
| unicode-width | 0.2.2 [06] | MIT OR Apache-2.0 [06] | correct cell widths for CJK/emoji filenames (Chinese paths render); column alignment + truncation via `UnicodeWidthStr::width()`; dua pulls 0.2 under its tui feature [06] | — | ui | unicode-segmentation 1.13.3 (grapheme clustering; needed too if truncating mid-cluster) [06] | ~~shelved (TUI→GUI pivot, ADR-5/18)~~ |
| signal-hook | 0.4.4 [06] | MIT OR Apache-2.0 [06] | real SIGINT/SIGTERM/SIGTSTP routing (belt-and-braces cancel path); crossterm itself depends on 0.3.17 for SIGWINCH [06] | mio integration optional [06] | ui, execute | `libc::sigaction` directly (unsafe, easy to get wrong) [06] | ~~shelved (TUI→GUI pivot, ADR-5/18)~~ |
| indicatif | 0.18.6 [04][06] | MIT [04][06] | progress bars for NON-TUI CLI mode only (plan/apply/purge subcommands); draws to stderr, max 20 refreshes/s, `abandon_with_message` on cancel; NEVER inside the ratatui loop [04][06] | — | ui (cli mode), plan | hand-rolled `\r`-updates [06]; ratatui `Gauge`/`LineGauge` in TUI [06] | retained — headless CLI progress (D-08 CLI subset kept; ADR-5/18 pivot) |

### 1d. Agent / LLM stack (feature `llm`)

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| reqwest | 0.13.4 (2026-05) [07] | MIT OR Apache-2.0 [07] | HTTP client; rustls DEFAULT since 0.13 (aws-lc-rs provider, roots via `rustls-platform-verifier`); built-in `ClientBuilder::retry(policy)` since 0.12.23; no native-tls/OpenSSL anywhere [07] | default = rustls; `query`/`form` now opt-in; enable `json` explicitly (verify at build time); `rustls-no-provider` if ring provider wanted [07] | agent | hyper + hyper-rustls [07] |
| async-openai | 0.41.3 (2026-07-31) [07] | MIT [07] | typed chat-completions client: tool-calling types, `CompletionUsage` (+ cached/reasoning token details), streaming usage chunk via `stream_options.include_usage`, `OpenAIConfig::with_api_base` (DeepSeek/OpenRouter/Ollama/vLLM), built-in `SimpleRetryPolicy` (429/5xx + Retry-After); default `rustls` [07] | `default = ["rustls"]`; fine-grained: `chat-completion`, `middleware` (on tower), `byot` — compile in only chat-completions to keep binary lean [07] | agent | hand-roll reqwest+serde_json [07] |
| reqwest-eventsource | 0.6.0 [07] | MIT OR Apache-2.0 [07] | SSE with auto-reconnect — only if hand-rolling chat JSON without async-openai; last release 2024-03, stable but aging [07] | — | agent (alternative path) | eventsource-stream + manual reconnect [07] |
| eventsource-stream | 0.2.3 [07] | MIT OR Apache-2.0 [07] | pure SSE Stream adapter used by async-openai internally; if using async-openai you never touch SSE crates; pick ONE of the two, never stack both [07] | — | agent (alternative path) | reqwest-eventsource [07] |
| tiktoken-rs | 0.12.0 [07] | MIT [07] | OpenAI-model token preflight estimates ONLY (`o200k_base`, `o200k_harmony`, `cl100k_base`); never for billing — DeepSeek/Ollama use own BPE, estimates labeled approximate [07] | per-encoding features [07] | agent | approx char/4 heuristic [07] |
| backon | 1.6.0 [07] | Apache-2.0 [07] | retry wrapper for tool calls/connect errors; `ExponentialBuilder`/`ConstantBuilder`/`FibonacciBuilder`, `.when()`/`.notify()`, Retry-After support [07] | — | agent | tower::retry [07] |
| tower | 0.5.3 [07] | MIT [07] | retry/budget layer primitives (async-openai middleware is built on it); pull in only if not using backon [07] | — | agent | backon [07] |
| tokio | 1.53.1 [07] | MIT OR Apache-2.0 [07] | async runtime for the agent loop only (reqwest/async-openai need it); need only `rt-multi-thread` + `macros` + `sync`; NO tokio in scan/ui/ledger paths [07] | `rt-multi-thread`, `macros`, `sync` — skip full/default [07] | agent | smol [07] |
| serde_json | 1.0.151 [08] | MIT OR Apache-2.0 [08] | wire-format (de)serialization + JSONL ledger records; canonical `to_vec` for CRC input; tool `arguments` is a JSON string [07][08] | — | agent, ledger, config | — |
| serde | 1.0.229 [08] | MIT OR Apache-2.0 [08] | derive for ledger/plan/config types (MIT OR Apache-2.0 per crates.io) [02][08] | `derive` | agent, ledger, config | — |

### 1e. Config / CLI / errors / misc

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| toml | — [INFERENCE: not in notes; pinned at write time] | — [INFERENCE] | config file parsing (dua reads config; dust ships `config.toml` precedent [01]) | `parse`/`display` as needed | config | serde + `toml_edit` (format-preserving) |
| anyhow | — [INFERENCE: not in notes] | — [INFERENCE] | application-level error context at CLI/plan boundaries | — | plan, config | hand-rolled error enums |
| thiserror | — [INFERENCE: not in notes] | — [INFERENCE] | typed error enums for library-ish modules (scan/classify/ledger) | — | scan, classify, ledger | manual `impl Error` |
| clap | — [INFERENCE: not in notes] | — [INFERENCE] | CLI subcommand parsing (scan/plan/apply/purge/restore + TUI) | `derive` | config | argh |
| chrono | — [INFERENCE: not in notes] | — [INFERENCE] | RFC3339 `ts` in ledger/WAL record envelopes [08] | `serde` | ledger, config | time crate |
| time | — | — | alternative to chrono for RFC3339 timestamps [08]; pick one | — | — (alt) | chrono |
| uuid | — [INFERENCE: not in notes] | — [INFERENCE] | WAL op-id / session-id / quarantine `<wal-op-id>/` subdir naming [05][08] | `v4` | ledger, execute | ulid |

> Crate rows marked [INFERENCE] are NOT version/license-verified in notes 04–08 — the notes were
> verified on 2026-08-30 for the crates listed above; for these, check crates.io at Cargo.toml
> authoring time and pin what it reports. No version/license claim here is fabricated.

### 1f. Platform-gated dependencies (notes 09–11, verified 2026-08-30)

Every row is `#[cfg]`-gated per OS; the `windows`/`winreg` crates do not compile their Win32
modules on other targets — keep the cfg at the module boundary [09 §crates].

#### Windows (`[target.'cfg(windows)'.dependencies]`)

| Crate | Ver (2026-08) | License | Purpose / verdict | Notes |
|---|---|---|---|---|
| `windows` | 0.62.2 [09][11] | MIT OR Apache-2.0 [09][11] | PRIMARY Win32 bindings: `SetFileInformationByHandle`/`FileRenameInfoEx` (no-clobber = omit `FILE_RENAME_FLAG_REPLACE_IF_EXISTS` 0x1; POSIX_SEMANTICS 0x2), `FlushFileBuffers` (WAL durability), `MoveFileExW` (pre-1709 fallback), `GetTempPathW`, `CreateFileW`, `BY_HANDLE_FILE_INFORMATION`, `FileIdBothDirInfo` bulk enumeration, `FOLDERID_*` | official microsoft-rs bindings; versions/licenses via crates.io API 2026-08-30 [09][11]; feature sets `Win32::Storage::FileSystem` + `Win32::System::Threading` [11] |
| `windows-sys` | 0.61.2 [09] | MIT OR Apache-2.0 [09] | raw-bindings alternative for the same calls — cheaper compile; same source of truth (std itself uses its rename constants) | pick ONE of windows/windows-sys; std's two-tier rename (MoveFileExW → FileRenameInfoEx) is the reference implementation [11] |
| `winreg` | 0.56.0 [09] | MIT [09] | enumerate `HKLM\…\Explorer\VolumeCaches` (cleanmgr handler discovery) + `HKCU\…\Lxss` `BasePath` (WSL vhdx locations) | thin registry wrapper |
| `same-file` | 1.0.6 [09][11] | Unlicense/MIT [09][11] | same-volume check via `BY_HANDLE_FILE_INFORMATION` volume-serial compare; `Handle` equality (volume serial + file index) works on UNC paths too | portable; also listed under cross-platform |
| `trash` (optional) | 5.2.6 [05][11] | MIT [05][11] | Windows backend = `IFileOperation` COM (`FOF_NO_UI \| FOF_ALLOWUNDO \| FOF_WANTNUKEWARNING`) — Recycle Bin as purge-time handoff ONLY, never the restore path | COM apartment feature flags `coinit_*` [11 §6] |

#### Linux (`[target.'cfg(target_os = "linux")'.dependencies]`)

| Crate | Ver (2026-08) | License | Purpose / verdict | Notes |
|---|---|---|---|---|
| `rustix` | 1.1.4 [05][10][11] | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT [05][10][11] | `fs::renameat_with` (`RenameFlags::NOREPLACE` — the syscall layer for `renameat2` where the glibc wrapper is missing), `fs::syncfs`, `fs::statx` (birth time via `StatxFlags::BTIME`) | libc-free syscall layer; already in the core table for xattr — same version, no new dep [10][11] |
| `libc` | 0.2.189 [04][05][08][10][11] | MIT OR Apache-2.0 [10][11] | `renameat2` fn + `RENAME_NOREPLACE=1`/`RENAME_EXCHANGE=2`/`RENAME_WHITEOUT=4` constants (gnu module) | already the core FFI dep; on Linux it gains renameat2/statx duty [11] |
| `nix` | 0.31.3 [05][08][10] | MIT [05][08][10] | `statvfs`, xattr, mount resolution, `renameat2` wrappers where libc provides them | platform-gate with `#[cfg(target_os = "linux")]` [10] |

Linux FS-matrix caveat [10 §0][11 §1]: `renameat2(RENAME_NOREPLACE)` support is kernel/FS-gated
(ext4 ≥3.15, btrfs/tmpfs ≥3.17, xfs ≥4.0, many more ≥4.9; **NFS: unsupported** — `nfs_rename()`
returns `-EINVAL` for any flags) ⇒ probe per volume via `/proc/self/mountinfo` fstype; on NFS
fall back to `O_TMPFILE`+`linkat` staging or refuse auto-action. glibc wrapper needs 2.28
(`_GNU_SOURCE`); raw `syscall(SYS_renameat2, …)` only for glibc < 2.28 (musl: wrapper since 1.1.24,
not re-verified).

#### Cross-platform (all targets; already in core table unless noted)

| Crate | Ver (2026-08) | License | Purpose / verdict | Notes |
|---|---|---|---|---|
| `same-file` | 1.0.6 [04][09][10][11] | Unlicense/MIT [09][10][11] | same-volume / hardlink identity: Unix `(st_dev, st_ino)` vs Windows `GetFileInformationByHandleEx(FileIdInfo)` `(volume_serial, file_index)` with documented file-index-aliasing fallback | prefer `Handle` equality over raw `st_dev` for quarantine-volume checks [11 §4] |
| `filetime` | 0.2.29 [11] | MIT/Apache-2.0 [11] | set/preserve mtime across move/restore | per-OS `src/unix/macos.rs` getattrlist fast path + utimensat fallback [11 §7] |
| `xattr` | 1.6.1 [05][11] | MIT OR Apache-2.0 [05][11] | evidence tags on quarantined files — **unix only** | Windows is compile-ok-but-unsupported (`SUPPORTED_PLATFORM=false`, io::Error at runtime) [11 §7] ⇒ ledger-based metadata, not fs-xattr [11 risks #5] |
| `trash` | 5.2.6 [05][11] | MIT [05][11] | OS trash per backend: macOS `NSFileManager`/Finder (delete-only), Windows `IFileOperation`, Linux FreeDesktop (`.trashinfo` replay via `os_limited::restore_all`) | crate msrv 1.85 [11]; FreeDesktop cross-topdir fallback = non-atomic copy+delete ⇒ never our restore path [11 §6/risks]; Linux getmntent UB note — pin calls to one thread [11 risks #4] |
| `dirs` | 6.0.0 [11] | MIT OR Apache-2.0 [11] | per-OS roots: `cache_dir` = `$XDG_CACHE_HOME|$HOME/.cache` / `{FOLDERID_LocalAppData}` / `~/Library/Caches` | `runtime_dir`/`executable_dir` are Linux-only (return `None` elsewhere) — gate by `Option` [11 §7] |

### 1g. GUI layer (Tauri + Svelte) — ADR-5/18 pivot

All Rust GUI host crates live in `src-tauri/Cargo.toml`, NOT the core workspace — they are
built as a separate Tauri app crate and pulled in via `tauri`'s own dependency graph (see §3
comment). Frontend packages are npm-managed (no Cargo involvement). Owning module: `ui/`.
Versions/licenses verified 2026-08-31 at crates.io / npm registry [12].

| Crate | Ver (2026-08-31) | License | Purpose / verdict | Feature flags | Notes |
|---|---|---|---|---|---|
| `tauri` | 2.11.5 [12] | Apache-2.0 OR MIT [12] | Rust app shell: `#[tauri::command]` (UI→Rust, typed via serde), event system (Rust→UI), `ipc::Channel<T>` (ordered streaming — scan/classify progress); `tauri::State` for a managed `ScanRegistry`; permissioned capabilities in `src-tauri/capabilities/*.json` (deny-by-default) [12] | `tray-icon` for menubar mode [12] | newest stable; all `tauri`-family crates Apache-2.0 OR MIT [12] |
| `wry` | 0.56.1 [12] | Apache-2.0 OR MIT [12] | system webview host (macOS WKWebView / Windows WebView2 / Linux WebKitGTK) — pulled transitively by `tauri` [12] | — | webview fragmentation risk per-OS [12 risks #1] |
| `tao` | 0.37.0 [12] | Apache-2.0 [12] | window/event abstraction under Tauri (traffic lights, resize, theme change) [12] | — | pulled transitively by `tauri` [12] |
| `tauri-plugin-dialog` | 2.7.3 [12] | Apache-2.0 OR MIT [12] | native open/save/message dialogs (sheet-style on macOS); used for volume pickers + confirmations, NOT scan I/O [12] | — | scan/apply/purge I/O stays in Rust commands (D-02/D-08) [12] |
| `tauri-plugin-fs` | 2.5.2 [12] | Apache-2.0 OR MIT [12] | scoped app-config fs for JS only (capability path-scope system); coarse volume-wide writes kept out of the webview [12] | — | decision: all scan/classify/plan/apply/purge I/O via Rust commands — preserves WAL + typed byte-count correctness [12] |
| `window-vibrancy` | 0.8.0 [12] | Apache-2.0 OR MIT [12] | macOS `apply_vibrancy` (NSVisualEffectMaterial, 10.10+) / macOS 26 `apply_liquid_glass` / Windows 11 Mica + 10 Acrylic; cfg-gated per OS [12] | — | requires `transparent: true` **and** `macOSPrivateApi: true`; private APIs block App Store acceptance (fine for direct-DMG; Linux unsupported — compositor-controlled) [12 risks #3] |

Frontend (npm, owning module `ui/`) — versions/licenses per note [12]:

| Package | Ver (2026-08-31) | License | Purpose |
|---|---|---|---|
| `svelte` | 5.57.0 [12] | MIT [12] | UI framework (runes: `$state`/`$derived`/`$effect`); compiles away, no VDOM — appends thousands of list rows/sec cheaply; compiler a11y warnings as CI errors [12] |
| `svelte-i18n` | 4.0.1 [12] | MIT [12] | CN-primary + EN-secondary labels (D-02/D-12); `$_('key')`, `init({ fallbackLocale })`, JSON locales in `src/lib/i18n/zh-CN.json` + `en.json` [12] |
| `typescript` | 7.0.2 [12] | Apache-2.0 [12] | type checking; TS major line 7 now on npm [12] |
| `@tauri-apps/api` | 2.11.1 [12] | Apache-2.0 OR MIT [12] | `invoke`/`Channel`/`event`/`window`/`menu`/`tray` from the frontend; framework-agnostic [12] |
| `@tauri-apps/cli` | 2.11.4 [12] | Apache-2.0 OR MIT [12] | dev/build/bundle CLI (`tauri build --bundles dmg\|msi\|appimage`) [12] |

> Supersedes the shelved TUI stack (§1c) per ADR-5/18 (D-09/D-10/D-11). ratatui/crossterm/
> unicode-width/signal-hook rows above are retained for history + ADR traceability, struck as
> shelved; `indicatif` stays (headless CLI subset, D-08). Rust core is unchanged (R1): the GUI
> layer is a pure presentation host over the same scan/classify/plan/execute/ledger/policy core.

## 2. Pitfalls table

| Pitfall | Detail | Source |
|---|---|---|
| jwalk is unmaintained | jwalk 0.9.0 README states "Unmaintained … use dua-core"; also no mount/device check anywhere in source, no gitignore, `skip_hidden` on by default — do NOT adopt even as fallback | [04] |
| dua-core is 10 days old | first standalone release Aug 2026, ~10k downloads; pin exact version; audit `crates/dua-lib/src/macos` unsafe code (raw `attrlist` buffers) before shipping; fallback = ignore::WalkParallel | [04] |
| indicatif never in TUI | indicatif draws to its own Terminal on stderr at 20 Hz and assumes it owns the output — inside a ratatui alternate-screen app the two fight over the terminal; none of the four studied TUI tools use it interactively; use ratatui `Gauge`/`LineGauge` + ticker-thread snapshot pattern instead; indicatif only in headless CLI subcommands | [04][06] |
| tokio for no benefit | dua AND gitui both stay on threads + crossbeam; ratatui FAQ says async is only worth it if the app already spawns many tasks; tokio::sync::mpsc "drags in tokio for no benefit here" — tokio ONLY behind the `llm` feature for reqwest/async-openai | [06][07] |
| Commons Clause license caveat | Pearcleaner 5.4.3 is Apache-2.0 + Commons Clause v1.0 — "Sell" prohibited; fine to READ for heuristics, do NOT vendor code into a monetized product | [02] |
| GPL copy-with-care | GrandPerspective GPL-2.0 (viral for linked code — reference only), git GPL-2.0-only (design reference only), xnu APSL-2.0 (reference only, never paste code) | [02][05][08] |
| ignore::WalkState::Quit is async | more entries may arrive after Quit; a switch to ignore::WalkParallel needs an explicit drain/flush phase; dua-core's drop-join avoids this bug class | [04] |
| crossterm major-version mixing | never let two crossterm major versions into one tree — separate event queues (races, lost events), raw mode tracked separately (may not restore on exit); pin 0.29 next to ratatui 0.30 and check transitive deps | [06] |
| two SSE layers | reqwest-eventsource and eventsource-stream are alternatives — choose one, do not stack both; if using async-openai you never touch SSE crates directly | [07] |
| tempfile gives atomicity, not durability | docs caveat: persist/persist_noclobber do NOT fsync file contents or the containing directory — sync(temp) + sync(parent dir) yourself (F_FULLFSYNC on Apple is std `sync_all()` since PR #60121) | [08] |
| crates.io API notes | versions/licenses above were pulled per-crate from the crates.io API on 2026-08-30 [04][07][08]; sysinfo 0.3x and indicatif 0.1x churn between minor versions — pin tightly; dua-core 3.3.0 published 2026-08-28 (check for newer before pinning); libc 1.0.0-alpha.4 exists but the stable line is 0.2.x — stay on 0.2; sled 1.0.0-alpha.124 stalled since 2024-10 | [04][07][08] |
| fs2 vs fs4 | fs4 is the maintained rustix-based fork (sync feature MSRV 1.75); fs2 0.4.3 is the older original — prefer fs4; both advisory-only locks | [08] |
| libc missing macOS symbols | `setiopolicy_np` NOT exported by libc (own `extern "C"` block, plain linking vs libSystem suffices); nix `FileFlag` lacks `SF_DATALESS`/`UF_DATAVAULT`/`SF_FIRMLINK`; nix has NO `renamex_np` (`RenameFlags` is Linux-only) — raw libc it is | [04][05] |
| rustix license triple | rustix is Apache-2.0 OR MIT OR Apache-2.0-WITH-LLVM-exception — unusual triple; fine for use, attribute correctly if vendoring | [05] |
| num_cpus vs std | `std::thread::available_parallelism` covers most needs; num_cpus only if you mirror dua's exact `threads` plumbing | [04] |
| signal-hook transitive version | we list signal-hook 0.4.4 while crossterm 0.29 pulls signal-hook 0.3.17 transitively — two minor lines in one tree. Harmless (they don't share types across our boundary; we use only signal-hook's own flag/mio surface), but check `cargo tree -d` at pin time and prefer routing through crossterm's own SIGWINCH handling where possible | [06] |
| Windows rename semantics | No-clobber = *omit* `FILE_RENAME_FLAG_REPLACE_IF_EXISTS` — `FILE_RENAME_FLAG_NO_REPLACE` does not exist; [MS-FSCC] documents no min OS version for `FileRenameInfoEx` (headers appear in Win10 1607; POSIX_SEMANTICS usable per Win10 1709, rust-lang#123985) ⇒ runtime probe + `MoveFileExW` fallback, never compile-time assumption; whether POSIX rename covers directory targets is undocumented; SMB shares have no POSIX rename semantics — treat as external volumes | [09 §delta/risks][11 §1/risks] |
| NTFS hardlink/junction specifics | `CreateHardLink` can't reference directories or cross volumes; junctions (mount-point reparse points) may cross local volumes — walker must never descend them; NTFS has no per-file CoW ⇒ freed-byte estimates rely on hardlinks only (no APFS-clone analog); supersede-disposition opens fail while an ADS is open | [09 §0][11 §4] |
| Linux renameat2 support matrix | RENAME_NOREPLACE is kernel/FS-gated (ext4 ≥3.15; btrfs/tmpfs/cifs ≥3.17; xfs ≥4.0; many more ≥4.9); NFS rejects ANY rename flags (`-EINVAL`) and its failure does not guarantee the file was not renamed — probe fstype per volume, fall back or refuse; glibc wrapper only ≥2.28 | [10 §0][11 §1] |
| FreeDesktop trash is not our restore path | cross-topdir trash = non-atomic copy+delete fallback (violates zero-bytes-freed); `.trashinfo` replay is the only restore the trash crate offers on Linux; use own quarantine + WAL instead — trash is purge-time handoff only; `trash` Linux backend uses non-threadsafe `getmntent` under Mutex — single-thread it | [11 §6/risks] |
| xattrs not portable for metadata | Linux `user.*` xattrs are per-fs (ext4/xfs/tmpfs yes, vfat no); Windows unsupported in the `xattr` crate; BSD flags (`st_flags`) exist in std on macOS only — Linux immutable = `FS_IOC_GETFLAGS` (out of note scope), Windows readonly = `FILE_ATTRIBUTE_READONLY` ⇒ restore metadata lives in the JSONL ledger only | [11 §7/risks] |
| long paths on Windows | classic MAX_PATH 260; long paths opt-in since Win10 1607, `\\?\` prefix for 32,767 wchar — manifest paths must carry `\\?\` form past 260 and rename targets validated | [09 §0] |
| jwalk is NOT macOS-only | its cross-platform reputation is stale in the other direction: jwalk 0.9.0 (2026-08-05) is fully portable (std::fs under the hood) — but remains REJECTED here as unmaintained per [04]; dua-core's per-OS native walkers (macOS getattrlistbulk, Windows FileIdBothDirInfo) are the fast-path design to copy | [09 §crates][11 §5] |

## 3. Proposed [dependencies] block

```toml
[dependencies]
# --- core (default feature: headless scan/classify/plan/execute/ledger) ---
dua-core = "=3.3.0"                      # [04] pin exact: 10-day-old standalone crate
ignore = "0.4.33"                        # [04] gitignore builder + fallback walker
globset = "0.4.20"                       # [04] policy/ignore glob matching
walkdir = "2.5.0"                        # [04] serial fallback traversal
crossbeam-channel = "0.5.16"             # [04][06] scan->ui events + select! loop
num_cpus = "1.17.0"                      # [04] worker counts (or std available_parallelism)
sysinfo = "0.39.6"                       # [04] volume list/capacity/removable flags
rayon = "1.12.0"                         # [04] CPU-bound classify parallel map ONLY
dashmap = { version = "6.2.1", optional = true }  # [04] only if classifier threads share maps
libc = "0.2.189"                         # [04][05][08] FFI: renamex_np, clonefile, F_FULLFSYNC...
nix = { version = "0.31.3", features = ["fs"] }   # [05][08] fcntl/fstatat/flock typed APIs
rustix = { version = "1.1.4", features = ["fs"] } # [05] xattr module
tempfile = "3.27.0"                      # [08] atomic persist for WAL compaction
crc32fast = "1.5.1"                      # [08] ledger/WAL per-record CRC
fs4 = "1.1.0"                            # [08] advisory flock (sync feature MSRV 1.75)
fs-err = "3.3.1"                         # [04][08] io error context on ledger/config IO
serde = { version = "1.0.229", features = ["derive"] } # [08]
serde_json = "1.0.151"                   # [08] JSONL ledger + CRC canonical bytes
toml = "0.9"                             # config parsing; 0.9 line current, 1.1.4 exists (spec-embedded, new) — stay on 0.9 for MSRV safety [verified 2026-08-31]
thiserror = "2.0.20"                     # typed module errors [verified 2026-08-31]
anyhow = "1.0.104"                       # CLI/plan boundary errors [verified 2026-08-31]
clap = { version = "4.6.6", features = ["derive"] }   # subcommands [verified 2026-08-31]
chrono = { version = "0.4.45", features = ["serde"] } # RFC3339 ts [verified 2026-08-31]
uuid = { version = "1.26.0", features = ["v4"] }      # WAL op ids [verified 2026-08-31]

# --- TUI (feature "tui") — SHELVED 2026-08-31 (GUI pivot, ADR-5/18): kept for reference; indicatif stays active for headless CLI ---
ratatui = { version = "0.30.2", optional = true } # [06] shelved
crossterm = { version = "0.29", optional = true } # [06] shelved (was ratatui hard dep)
unicode-width = { version = "0.2.2", optional = true } # [06] shelved
signal-hook = { version = "0.4.4", optional = true }   # [06] shelved
indicatif = { version = "0.18.6", optional = true }    # [04][06] ACTIVE — headless CLI progress ONLY (ADR-14)
# --- GUI (Tauri 2.11.5) lives in src-tauri/Cargo.toml, not this core workspace — see §1g (ADR-5/18) ---

# --- LLM agent loop (feature "llm") ---
tokio = { version = "1.53.1", optional = true, features = ["rt-multi-thread", "macros", "sync"] } # [07]
reqwest = { version = "0.13.4", optional = true, default-features = true, features = ["json"] }   # [07] rustls default
async-openai = { version = "0.41.3", optional = true, features = ["chat-completion", "middleware"] } # [07]
reqwest-eventsource = { version = "0.6.0", optional = true }  # [07] only if hand-rolling SSE (pick ONE)
eventsource-stream = { version = "0.2.3", optional = true }   # [07] alt to reqwest-eventsource (pick ONE)
tiktoken-rs = { version = "0.12.0", optional = true } # [07] preflight estimates, never billing
backon = { version = "1.6.0", optional = true }       # [07] tool-call retry
cargo_metadata = "0.23.1"                # [SPEC Part B §3.2] resolve toolchain/target layout for classify evidence
regex = "1.13.1"                         # [SPEC Part B §3.6] policy compiler token patterns
dirs = "6.0.0"                           # [SPEC Part B §3.9] XDG/home dir resolution for config + caches
secrecy = "0.10"                         # [SPEC Part B §3.9] API-key type (no accidental Debug/Display leaks)
same-file = "1.0.6"                      # [SPEC Part B §2.4] st_dev/mntonname dev-identity checks
unicode-segmentation = "1.13.3"          # [06] grapheme-safe truncation for CJK/emoji filenames

# --- platform-gated (notes 09-11) — see §1f for the full version/license table ---
[target.'cfg(windows)'.dependencies]
windows = { version = "0.62.2", features = ["Win32_Storage_FileSystem", "Win32_System_Threading"] } # [09][11] FileRenameInfoEx, FlushFileBuffers, FileIdBothDirInfo
windows-sys = "0.61.2"                   # [09] raw-binding alternative (pick ONE of the two)
winreg = "0.56.0"                        # [09] VolumeCaches + Lxss BasePath enumeration
filetime = "0.2.29"                      # [11] mtime preservation across move/restore (all targets)

[target.'cfg(target_os = "linux")'.dependencies]
# rustix/libc/nix already in core deps; on Linux they gain renameat2/syncfs/statx duty [10][11]
filetime = "0.2.29"                      # [11] utimensat path

[target.'cfg(unix)'.dependencies]
xattr = "1.6.1"                          # [05][11] evidence tags (unix only; Windows unsupported)
```

Notes: `dashmap` stays optional until classifier threads provably need shared maps [04];
`reqwest-eventsource` vs `eventsource-stream` — enable exactly one [07]; keep SSE crates disabled
while async-openai parses streams itself [07]. Platform-gated blocks follow §1f: `windows`/
`windows-sys`/`winreg` must stay inside `[target.'cfg(windows)']` (their Win32 modules do not
compile elsewhere) [09 §crates]; pick `windows` OR `windows-sys`, never both [09];
`filetime`/`xattr`/`same-file`/`dirs` are listed once per gate only where their surface differs.

## 4. Feature-gating strategy

```toml
[features]
default = []            # headless core: scan/classify/plan/execute/ledger/policy/config
tui = ["dep:ratatui", "dep:crossterm", "dep:unicode-width", "dep:signal-hook", "dep:indicatif"]
llm = ["dep:tokio", "dep:reqwest", "dep:async-openai", "dep:tiktoken-rs", "dep:backon"]
```

- **default = headless core.** scan → dua-core walk + crossbeam events; classify → rayon CPU pool;
  plan/execute → nix/libc quarantine moves; ledger → JSONL + crc32fast + fs4 flock + tempfile
  compaction; policy/config → globset + toml + clap. Zero async runtime, zero terminal control.
  CLI progress without TUI comes via `indicatif` (kept in the `tui` feature so the pure-headless
  build stays lean; move to `default` if headless builds still need progress bars).
- **`tui` feature**: ratatui 0.30.2 + crossterm 0.29 pinned together [06]; unicode-width for CJK
  filenames [06]; signal-hook for external SIGINT/SIGTERM routing [06]; NO tokio in this path [06].
- **`llm` feature**: the only place tokio lives [06][07]; reqwest (rustls default) + async-openai
  with only `chat-completion` + `middleware` features to keep the binary lean [07]; tiktoken-rs for
  preflight token estimates (never billing) [07]; backon for tool-call retry [07]. SSE crates
  (reqwest-eventsource OR eventsource-stream) stay OUT of the feature until hand-rolling is chosen
  — async-openai already parses streams [07].
- Both features compose: `tui` + `llm` = the interactive agent UI; the privacy chokepoint (tool
  serializer) lives in `agent/` regardless of features [07].
- CI should build `--no-default-features`, default, `--features tui`, `--features llm`, and both,
  so crossterm/ratatui version drift is caught at the edges [06].

## 5. Version risk register (churn before 2026-09-08)

| Dependency | Risk | Mitigation |
|---|---|---|
| dua-core 3.3.0 | 10-day-old standalone crate (2026-08-28, ~10k downloads) — most likely to churn before 2026-09-08 | Pin exact `=3.3.0` [04]; watch crates.io for 3.3.1+; audit macOS unsafe code; fallback = ignore::WalkParallel + own channel + `std::thread::available_parallelism()` [04] |
| ratatui 0.30.2 / crossterm 0.29 | crossterm 0.30 does not exist yet as a known target; when a new major lands, ratatui moves (non-optional dep) and every direct crossterm use must move in the same commit (separate event queues per major = race/lost-event risk) | Pin `crossterm = "0.29"` next to `ratatui = "0.30"`; check transitive deps in CI [06] |
| reqwest 0.13.x | 0.13.0 broke with rustls-default (crypto provider aws-lc-rs); `query`/`form` became opt-in; `json` default-on "verify at build time" | Pin `0.13.4`; enable `json` explicitly; test base-URL overrides (DeepSeek/OpenRouter/Ollama/vLLM) at build time [07] |
| async-openai 0.41.3 | fine-grained feature churn (`chat-completion`, `middleware`, `byot`); 0.41.3 released 2026-07-31 | Pin exact; compile only chat-completions [07] |
| sysinfo 0.39.6 | 0.3x "churns between minor versions" | Pin exact; `DiskRefreshKind` to trim cost [04] |
| indicatif 0.18.6 | 0.1x minor-version churn | Pin exact [04] |
| libc 0.2.x | 1.0.0-alpha.4 pre-releases exist; stable line is 0.2.x | Stay on `0.2.x` (0.2.189); do NOT adopt 1.0-alpha [04][05][08] |
| tokio 1.53.1 | minor-version drift normal | `1.53.1` with minimal features [07] |
| fs4 1.1.0 | fork-of-fs2 lineage; sync feature MSRV 1.75 | Pin 1.1.0; verify MSRV fits toolchain [08] |
| nix 0.31.3 | 0.x line; API surface (fcntl enums, Flock) stable-ish but 0.x | Pin 0.31.3 [05][08] |
| tempfile 3.27.0 | persist semantics changed at 3.18.0 (renameat_with on Apple) | Pin ≥3.27.0; re-verify docs caveat (no fsync) on upgrade [08] |
| General pinning advice | Any crate could publish tomorrow | For young/churning crates (dua-core, sysinfo, indicatif, reqwest, async-openai) pin EXACT versions in Cargo.toml and let only deliberate upgrades move them; for ultra-stable lines (serde 1.x, libc 0.2.x, crossbeam-channel 0.5.x, globset 0.4.x, walkdir 2.x) caret ranges are acceptable [04][07][08]. Commit Cargo.lock; re-verify pinned versions against crates.io API the day Cargo.toml lands (all claims here verified 2026-08-30 [04]–[08], 2026-08-31 [09]–[13] + §3 INFERENCE batch) |
