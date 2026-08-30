# Disk Janitor — SPEC (draft v0)

> Status: **draft**. Seed document distilled from the posted topic (2026-08-27) and
> course requirements. Iterate this into `docs/DESIGN.md` as implementation decisions
> land; keep it in sync with the code (course rule).

## 1. Positioning

A macOS-only, developer-focused disk-cleanup **Agent**. It answers the question
`du` can't: not "where are the bytes?" but "**which of these are safe to reclaim,
why, and how do I get them back if I'm wrong?**"

- Users: developers on capacity-starved Macs (Arys: 16 GB MacBook 飘红常客;
  classmates with 512 GB drives on 2-week cleaning cycles).
- Boundary: it only services the disk-cleanup scenario. It is not a general file
  manager and not a generic chat wrapper.

## 2. Pain-point analysis (why existing tools fail)

| Tool | What it gives | What it lacks |
|---|---|---|
| `du` / `ncdu` | byte locations | no safety judgment; no memory between runs |
| DaisyDisk | pretty maps | undo depends on Trash not being emptied |
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
   - `apply`: atomic same-volume move into an on-volume quarantine dir via
     `renamex_np(RENAME_EXCL)` — zero bytes freed, fully reversible.
   - `purge`: actually frees space; requires typing the exact byte count.
   - `restore`: one command, full restoration within the 72 h grace window.
   - WAL (write-ahead log) ⇒ crash-recoverable; 删除是操作，恢复是权利.
3. **(bonus) NL policy compiler** — "腾出 25GB，别动我这两周在跑的项目" compiles to
   structured planner constraints. **Privacy invariant: the LLM never sees file
   paths or names — only the intent sentence and aggregate statistics.**

## 4. Architecture sketch

```
src/
  scan/      walker (parallel, metadata-first: size/mtime/flags)
  classify/  evidence engine + rebuild-cost grading (rule engine, local)
  plan/      candidate selection under policy constraints → JSON manifest
  execute/   quarantine (renamex_np) / purge / restore, WAL
  ledger/    persistent JSONL 台账: every verdict & operation, cross-session memory
  policy/    NL → structured constraints (local rules; optional LLM)
  agent/     agent loop: model ↔ tools (scan/classify/plan/apply), stop conditions,
             token budget enforcement
  ui/        ratatui TUI: evidence review, progress, ledger trajectory
  config/    config.toml: endpoint, key, model, context length, thinking mode,
             API prices, budget
```

Key types (working names): `Candidate { path, size, class, evidence: Vec<Evidence> }`,
`Evidence { kind, detail, strength }`, `PlanEntry`, `LedgerRecord`, `WalEntry`.

## 5. R1–R6 mapping (功能完整性, 30 pts)

| Req | Implementation |
|---|---|
| R1 Rust core | all of the above; syscall layer via `nix`/`libc` |
| R2 UI | ratatui interactive TUI (CLI subcommands as fallback) |
| R3 model config | `config.toml` + TUI settings: endpoint, key, model, ctx len, thinking, prices |
| R4 progress + interrupt | live scan/classify progress bars; ctrl-c safe (WAL-consistent) |
| R5 context/history | JSONL ledger + session save/load; agent-loop trajectory visible in TUI |
| R6 tokens & cost | per-call usage captured → running cost at configured prices; budget ⇒ interrupt |

## 6. Milestones (course deadlines: 9/1 comments, 9/6 public demo, 9/8 tryouts, 9/10 presentation)

- M0 scaffold — repo, agent files, spec (this commit).
- M1 scan + classify core w/ fixture-tree tests.
- M2 plan/apply/purge/restore + WAL + ledger.
- M3 TUI (evidence review, progress, interrupt).
- M4 agent loop + NL policy compiler + token accounting.
- M5 demo script, README polish, design doc final, AI-log export, 开销明细表.

## 7. Open questions / risks (owner flagged these as gut-feel)

- Classification thresholds (e.g. "mtime < 10 min ⇒ actively written") — need tuning.
- Grace period: is 72 h right?
- APFS snapshot interactions; iCloud placeholder files (dataless) — scan must not
  trigger evict/download; classify must mark them appropriately.
- Purge semantics on external volumes (quarantine must stay same-volume to be atomic).

## 8. Explicit non-goals

Windows/Linux support; generic file search; content indexing; auto-delete daemon
(monitoring/reminder maybe later per classmate feedback).
