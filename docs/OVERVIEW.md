# Disk Janitor — Project Overview

> **The front door.** If you read only one doc, read this. Last updated 2026-08-31.

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
[`docs/UX-SPEC.md`](UX-SPEC.md).

## The 30-second architecture

```
┌────────────────────────── Tauri + Svelte App (UX-SPEC.md) ──────────────────────┐
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

## Document map (what lives where)

| Read order | Doc | What it gives you |
|---|---|---|
| 1 | **this file** | vision, architecture, status |
| 2 | [`SPEC.md`](SPEC.md) | product spec: positioning, scenario customizations, R1–R6 rubric map |
| 3 | [`UX-SPEC.md`](UX-SPEC.md) | the app itself: screens, wireframes, flows, visual language, copy |
| 4 | [`DESIGN.md`](DESIGN.md) | build-ready technical design: types, threads, syscalls, tests |
| 5 | [`design-decisions.md`](design-decisions.md) | 17 ADRs — every architectural decision + owner verdicts |
| 6 | [`crate-map.md`](crate-map.md) | every dependency: version, license, why (paste-ready Cargo.toml) |
| 7 | [`ecosystem-survey.md`](ecosystem-survey.md) | competitors: what we copy, adapt, reject (16 tools) |
| 8 | [`decision-register.md`](decision-register.md) | the 22 owner decisions from the 08-31 grill (verbatim answers + landing sites) |
| 9 | [`research/`](research/) | 13 primary-source notes backing every claim above |

**How we got here.** Conceived 2026-08-27 from the course topic post; scaffold 08-30
(agent files, spec); a 13-note verified research corpus and 12 technical ADRs landed
08-30 (`7a91240`). On 08-30 the owner expanded scope to **native 3-platform** (notes
09–11, ADR-11/12, `102fef2`). A 3-round grill on 08-31 surfaced 28 design gaps and
produced 22 owner decisions — the pivotal one: **TUI → GUI-first Tauri desktop App**
(UX-SPEC, ADR-5/13/14/16/17/18, `8308fcf`). Honesty-pass fixes landed same day
(`abc23f2`, `88b8dbe`); this overview + entry-point sync is `f0fcaa6`. Implementation
(M1) awaits the owner's go.

## Where we are (2026-08-31)

| Phase | Status |
|---|---|
| Research (13 notes) | ✅ done — licenses/versions verified at source |
| Architecture (17 ADRs) | ✅ 12 owner-stamped · ⏳ 5 awaiting owner line-review (13/14/16/17/18) |
| UI/UX design | ✅ UX-SPEC complete (wireframes, flows, purge ceremony) · ⏳ visual mockups open |
| **Implementation** | ❌ **0% — starts at M1 on owner's go** |
| Demo video + 9/10 presentation | ❌ not started (competitor research ready, note 13) |
| **9/1 design comments** | ⏳ docs complete as of 2026-08-31 — owner review & submit **by 9/1** |

## Milestones & deadlines

| M | Scope | Deadline |
|---|---|---|
| M1 | scan + classify core, fixture tests | — |
| M2 | plan/apply/purge/restore + WAL + ledger; Linux platform impls | — |
| M3 | Tauri GUI shell (Dashboard/Review/Plan/Purge) | **demo 9/6** |
| M4 | agent loop + NL policy + token/cost accounting | tryouts 9/8 |
| M5 | video, docs final, 开销明细表 | **present 9/10** |

Course requirement R2 (互动界面) is satisfied by the 桌面 App; R1–R6 mapping in SPEC §5.
