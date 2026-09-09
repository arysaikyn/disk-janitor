# 还我磁盘 Giveback — total redesign, PLAN pass

## Generic elements found (screenshot audit, 5 surfaces)

| # | Element | Where | Why generic |
|---|---|---|---|
| 1 | Flat indeterminate ring + "0%"→"…" label | Scan | A spinner with a number is a loading bar wearing a costume; no personality |
| 2 | Big-number hero card (`30.9 GB 可回收` in a rounded box) | Dashboard, Review | The hero-metric template: big number + label + stats row + accent. SaaS cliché |
| 3 | Full-width ghost buttons (取消 / 开始扫描 / 一键清理) | everywhere | Uniform rounded rect, no pressed state, no life; dead rect at rest |
| 4 | Table rows as bordered list (审核 judgment rows) | Review | Bordered zebra table = settings admin, not a product moment |
| 5 | Em-dash placeholder for snapshot-held | Dashboard | Dead end: no answer to "what is this", no affordance |
| 6 | Ledger empty state = one gray line 还没有记录 | Ledger | Zero warmth, zero brand; a dead end |
| 7 | Settings = right drawer with plain radios | Settings | OS settings clone; radio rows unlabeled groups; 卸载... truncated label |
| 8 | Grade dots (8px circles) | Review | Fine as data, but rows carry no other life — the table IS the screen |
| 9 | No motion anywhere: navigation is a hard cut, rows pop in, zero feedback | all | "Feels like operating" — nothing answers |
| 10 | Progress bar on dashboard = static gradient strip | Dashboard | Decoration, not information |

## Direction (≤5 lines)

- **Audience**: non-technical Mac owners whose disk is full and who are scared of deleting; course demo audience second.
- **Single job**: review → decide → reclaim, with the *undo promise* visible at every step (0 已销毁 until you confirm).
- **Palette (dark, committed)**: ink `#0A0E14` · surface `#111721` · ink-text `#E8EDF4` · mist `#8B98A8` · **restore-teal `#2DD4BF`** (the one accent; ~8% of surface) · grade dots stay data-only (green/teal/blue/amber/red).
- **Type**: SF Pro Text (UI) + SF Mono tabular-nums for every number — numbers ARE the interface. No display face; hierarchy via weight steps (600/400) and size jumps ≥1.33×.
- **Signature metaphor: the 呼吸球 (breathing orb)** — one living sphere that is the scan/clean/undo engine. Idle: breathes slowly (8s cycle, 2% scale). Working: spins up — orb compresses and orbits particles (scanned entries) spiral INTO it, rate = live scan rate; surface ripples per batch. Success: orb swells once (1.12×, 220ms ease-out) and exhales a teal pulse ring; purged bytes tick up beside it. Error: orb deflates 8% and flickers amber twice. Undo (restore): orb sucks the pulse ring BACK in — deletion literally reverses. It replaces the scan ring, the dashboard hero, the purge-gate visual, every progress bar. Without a label you understand: big motion = working; still + glow = done; suck-back = undone.
- **Motion rules**: 150–250ms standard, ease-out (cubic-bezier(0.2, 0, 0, 1)); exits 20% faster; everything interruptible (CSS transitions, not one-shot keyframes); nothing >300ms except the orb's continuous breath. Buttons scale 0.96 on press. Rows stagger-enter 40ms each, 8-row cap. `prefers-reduced-motion`: orb becomes a static state-dot (gray=working, teal=done, amber=error) with a crossfade; staggers collapse to instant.

## Slop self-check

- warm cream + serif + terracotta: no (dark committed ink + teal).
- dark bg + acid green: teal is desaturated mint-teal, not acid; text stays near-white.
- SaaS-card kit: killing the hero cards entirely — the orb + a single hairline replace boxed metrics.
- ALL-CAPS eyebrows / middots / numbered sections: none.
- fade-and-slide-up everywhere: rows stagger only in lists; no section reveals.
- hover-on-every-card: hover only on interactive rows/buttons.
- **"Would any other app get this exact design?"** No — the orb-as-engine (particles spiral in at live scan rate; undo = suck-back) is specific to an undo-first cleaner. A generic cleaner app shows a progress bar; this shows the machine eating and spitting back.

## Build order

1. Token system: CSS vars (palette above) + motion tokens (`--t-fast:150ms --t-med:220ms --ease:cubic-bezier(.2,0,0,1)`) + elevation (shadow layers, no borders) + radii (outer = inner + padding).
2. Components: Orb.svelte (the signature — polished most), PressButton (scale-0.96 + state colors), Row (interactive, stagger, no borders — hairline separators only), MetricLine (replaces hero cards: label + tabular number, no box), EmptyState (orb mini + one warm sentence).
3. Copy: "30.9 GB 可回收" → keep the number, kill the box; success = "已释放 1.2 GB · 214 个文件消失"; errors keep 判定+证据+替代.
4. Screens: Dashboard (orb idle+hero), Scan (orb working IS the ring), Review (rows, hairlines, orb mini per grade?), Plan, Ledger (orb empty state + restore rows), Settings (untruncated, grouped), PurgeGate (orb swell on confirm).
