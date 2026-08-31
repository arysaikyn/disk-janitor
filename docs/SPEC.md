# Disk Janitor — SPEC (draft v0)

> Status: **draft**. Seed document distilled from the posted topic (2026-08-27) and
> course requirements. Keep this document in sync with the implementation (course rule).
> Implementation design lives in `docs/DESIGN.md` (architecture), `docs/crate-map.md`
> (dependencies), `docs/ecosystem-survey.md` (reuse landscape), `docs/design-decisions.md`
> (ADRs); raw evidence in `docs/research/` (11 notes, versions/licenses verified 2026-08-30;
> notes 09–11 cover the Windows/Linux/platform-primitives research behind the 3-platform design).

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
  ui/        Tauri + Svelte desktop App (UX-SPEC.md): evidence review, progress,
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
| R2 UI | Tauri + Svelte 桌面 App (course R2 permits 桌面 App; UX-SPEC.md); thin CLI plumbing |
| R3 model config | `config.toml` + App settings pane: endpoint, key, model, ctx len, thinking, prices |
| R4 progress + interrupt | live scan/classify progress (ipc::Channel); interrupt-safe (WAL-consistent; apply auto-resumes, purge runs to completion) |
| R5 context/history | JSONL ledger + session save/load; agent-loop trajectory visible in App (Ledger screen) |
| R6 tokens & cost | per-call usage captured → running cost at configured prices; budget ⇒ interrupt |

## 6. Milestones (course deadlines: 9/1 comments, 9/6 public demo, 9/8 tryouts, 9/10 presentation)

- M0 scaffold — repo, agent files, spec (this commit).
- M1 scan + classify core w/ fixture-tree tests.
- M2 plan/apply/purge/restore + WAL + ledger.
- M3 GUI shell per UX-SPEC.md (Tauri + Svelte): evidence review, progress, interrupt.
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
promised is identical atomicity on every platform — the honest guarantee:

| Platform | Atomic no-clobber quarantine move | Caveats |
|---|---|---|
| macOS | full atomicity — `renamex_np(RENAME_EXCL)` [05 §1] | exFAT/FAT lack `VOL_CAP_INT_RENAME_EXCL` (probe + refuse) [05 §1] |
| Linux | atomicity on kernels ≥ 3.15 with `renameat2(RENAME_NOREPLACE)`, fs-dependent (ext4 ≥3.15, btrfs/tmpfs ≥3.17, xfs ≥4.0, many more ≥4.9) [10 §0][11 §1] | NFS rejects `RENAME_NOREPLACE` (`EINVAL`) — probe per volume, fall back or refuse [11 §1/risks] |
| Windows | POSIX-semantics rename via `SetFileInformationByHandle(FileRenameInfoEx)` (Win10 1709+; 1607 headers) [09 §delta][11 §1] | no explicit min-version in [MS-FSCC] — runtime probe, `MoveFileExW` fallback is weaker (non-atomic w.r.t. open targets); no dir-fsync (NTFS journal) [09 §delta][11 §1/§2] |
