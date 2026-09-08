# 还我磁盘 — Visual Mockup (interactive)

Single-file interactive visual mockup of the full product, implementing SPEC Part C
(UX design). Source of the visual language for the Svelte implementation (M3).
**Brand: 还我磁盘 / Giveback (EN coinage, owner pick 09-08).**

**Design language v2 (09-08)** — rework after owner critique: "not clean, text soup,
bilingual mashed, bland". v2 rules: one hero visual per screen · type-driven hierarchy
(numbers, not sentences) · color = grade data only, never decoration · **single-language
mode with a 中/EN switcher** (replaces the old dual-label rendering) · quiet deep-navy
chrome (`#0c1118` + one restore-teal accent) · microcopy cut to labels + hover hints.

## Open it

```bash
open docs/mockups/index.html        # macOS
xdg-open docs/mockups/index.html    # Linux
```

## What's inside — screens (SPEC Part C §3)

| Screen | Mockup state | SPEC ref |
|---|---|---|
| 仪表盘 Dashboard | volume bar, hero ring + 19.3 GB number, 「0 已销毁」, primary CTA, disabled one-tap, TCC state | §3.2 |
| 扫描 Scan | progress ring 63%, rate, current root, candidates + root errs, cancel | §3.3 |
| 审核 Review — the heart | **exception-centered**: 17.1 GB safe collapsed to one line, 4 judgment rows with grade dot + word, evidence expands inline (mtime/lock/regen), refusals welded | §3.4 |
| 计划 Plan | manifest: quarantine/prune/refused counts + sizes, dedup total, 72 h grace, soft-gate apply | §3.5 |
| 活动 Activity | exec rows, 「释放 0 字节」 live counter, WAL-resume | §3.6 |
| 台账 Ledger | record stream (restore/apply/plan/purge/budget), restore affordance, purge row opens gate | §3.7 |
| 助手 Agent dock (⌘J) | propose-only: plan card, only CTA 「在计划中打开」, budget/offline states | §3.9 |
| 设置 Settings sheet | scan defaults, agent config (R3), appearance, privacy, uninstall | §3.8 |
| Purge gate modal | typed exact byte count, Esc inert | §4.2 |

## Design decisions encoded (from SPEC Part C + note 21)

- **Calm deep-navy chrome, Apple-native** (DaisyDisk P5 restraint, [12] recipe):
  `#0c1118` base, hairline separators not card boxes, real traffic lights, content
  under titlebar, vibrancy-compatible surfaces.
- **Single-language mode** (owner decision 09-08, amends ADR-13 rendering): 中/EN
  switcher in the titlebar swaps the whole UI — one language at a time. The course's
  CN-primary requirement is met by defaulting to 中文; EN is a full translation, not
  inline duplication.
- **Type is the hierarchy**: `num` mono/tabular-nums for all byte counts; 10–12 px
  labels; one 34–40 px hero number per screen. Prose removed from surfaces — evidence,
  refusals, and caveats live in expandable details and hover hints.
- **Color means data only**: the five grade dots (pure/rebuild/redl/expensive/hands)
  appear exclusively on rows that carry a grade; the single accent is restore-teal —
  used for primary actions and the hero, never as decoration.
- **Anti-fear voice** (§1): 「0 已销毁」 hero, zero telemetry in sidebar, refusals
  rendered welded-visible, never hidden.
- **Reversibility made numeric**: 「释放 0 字节」 live during apply; typed byte count
  gates purge; grace countdowns in ledger.

## Theme

Follow system by default (`prefers-color-scheme`); in-app override 浅/深/跟随系统 in
Settings (§5.5). Grade dots are CSS variables per theme; glyph + word always present.

## Degraded states (verified 09-08)

All five spec'd degraded states are rendered (keyboard demo: `T` TCC banner,
`B` budget-reached, `U` agent-unreachable, `E` empty Review; evidence-unreadable row
lives inside Review's judgment list):

- TCC pre-flight banner (§4.1) — 判定 EPERM + evidence + [打开系统设置] / 排除并继续
- Evidence-unreadable row — ⚠ probe EPERM, refused-not-hidden
- Budget-reached banner (ADR-9) — stop reason + un-run turn count
- Agent-unreachable (P3) — 「助手不可用, 本地流程不受影响」
- Empty Review — 「扫描完成 — 没有值得回收的东西」 (good news, not failure)

## App icon

`docs/mockups/icon.svg` — the undo moment is the logo (note 20 direction B, D-23):
restore arc (↩) returning a byte-counter stack, calm dark navy tile. No broom, no
dust — restore, not scrub. Seed for `src-tauri/icons/` at M3.

## Scan-result persistence

Owner decision 09-08 (SPEC Part C §2.3): the last scan result is persisted and
re-openable (`get_last_scan` seeds Review/Plan without rescanning; a new scan replaces
it). Borrowed from the Mole/grandperspectiv pattern [03 §rec].

## Review-screen redesign (2026-09-08)

The heart screen was reworked from a candidate table to **exception-centered review**.
Owner picked it after: (1) ADHD divergent ideation (5 frames, 30 ideas — rejected breaker
panel, DaisyDisk map, collector cart), (2) competitor research showing the newest cohort
(Clean Mind, DiskX, SpaceLint) already does evidence+regen+reversible in a list, and the
only open lane is a review that isn't a list or a map.

Rationale: matches how non-technical users think ("just tell me what needs me"); the
safety grades carry the verdict so green/teal need no human walk; refusals stay
visible-not-hidden as welded rows with 判定→证据→替代. Reversibility lives in Activity/
Ledger (note 21 P3), not forced onto the review surface.

## Theme

Follow system by default (`prefers-color-scheme`); in-app override 浅 Light / 深 Dark
in Settings (§5.5). Grade colors are CSS variables per theme; glyph + word always
present so meaning never rides on hue alone.

## Translation to Svelte (M3)

This file's tokens (`--bg/--surface/--border/--fg/--accent`, grade classes
`b-green/b-teal/b-blue/b-amber/b-red`) map 1:1 to CSS custom properties in the Svelte
shell; components from §6 (`AppShell`, `Sidebar`, `GradeBadge`, `BilingualLabel`,
`VolumeCard`, `ScanProgressRing`, `CandidateTable`, `EvidencePanel`, `RefusalRow`,
`PlanManifest`, `ActivityList`, `LedgerView`, `PurgeGateModal`, `AgentDock`,
`SettingsSheet`) consume them. Store discipline (§6): Channel-fed `$state`, never poll.
