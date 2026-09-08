# 还我磁盘 — Visual Mockup (interactive)

Single-file interactive visual mockup of the full product, implementing SPEC Part C
(UX design). Source of the visual language for the Svelte implementation (M3).
**Brand: 还我磁盘 / Giveback (EN coinage, owner pick 09-08).**

## Open it

```bash
open docs/mockups/index.html        # macOS
xdg-open docs/mockups/index.html    # Linux
```

## What's inside — screens (SPEC Part C §3)

| Screen | Mockup state | SPEC ref |
|---|---|---|
| 仪表盘 Dashboard | volume truth (used/raw-free/snapshot-held), scan summary, hero 「0 已销毁」, primary CTA, disabled one-tap, first-run empty state | §3.2 |
| 扫描 Scan | progress ring 63%, live rate, current root, candidates + root errs, cancel semantics | §3.3 |
| 审核 Review — the heart | **exception-centered** (owner pick 09-08): answers "what needs your judgment?". Green/teal (210 items, 17.1 GB) shown as already-handled grouped lines with a collapsible evidence drawer; focus is the short list that needs a human call — Expensive opt-ins (⚠, per-entry include toggle) and HandsOff/refusals (🔒 welded rows) — each with 判定→证据→替代 inline. Headline 「19.3 GB 可回收 · 0 已销毁」. | §3.4 |
| 计划 Plan | manifest card: actions (quarantine/prune/refused), dedup total, 72 h grace, snapshot note, soft-gate apply | §3.5 |
| 活动 Activity | per-entry exec rows, 「释放 freed 0 字节」 live counter, WAL-resume banner, cancel | §3.6 |
| 台账 Ledger | record stream (restore/apply/plan/purge/budget), restore affordance, typed-byte-count purge row opens gate | §3.7 |
| 助手 Agent dock (⌘J) | propose-only chat: plan card with the one CTA 「在计划中打开」, refusal + evidence + alternative, cost/token footer | §3.9 |
| 设置 Settings sheet | scan defaults, agent config (R3), language, appearance, privacy, uninstall | §3.8 |
| Purge gate modal | typed exact byte count, Esc/outside inert, no cancel once confirmed | §4.2 |

## Design decisions encoded (from SPEC Part C + note 21)

- **Calm dark navy, Apple-native** (DaisyDisk P5 restraint, [12] recipe): `#0d1117`
  base, `#141a23` surfaces, vibrancy-compatible translucency, real traffic lights,
  content under titlebar.
- **Bilingual CN-primary** (ADR-13): every control is 「中文 English」, dense rows
  drop the EN tail — never mixes CN-only/EN-only.
- **Grade color language** (§3.1): green PureCache / teal Rebuildable / blue
  Redownloadable / amber Expensive / red HandsOff — each badge = glyph + word +
  color (colorblind-safe), CSS variables per theme.
- **Anti-fear voice** (§1): 「0 已销毁 until you confirm」 hero, zero telemetry in
  sidebar, refusal rows rendered disabled — never hidden.
- **Reversibility made numeric**: 「释放 freed 0 字节」 live during apply; typed
  byte count gates purge; grace countdowns in ledger.
- **Restore is the brand**: 恢复是权利 in sidebar foot; undo moment is the only
  primary CTA shape.

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
