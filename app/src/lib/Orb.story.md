# Orb — 呼吸球 interaction spec (story)

The signature. One living sphere that IS the scan/clean/undo engine
(`docs/redesign-PLAN.md`). Full component: `app/src/lib/Orb.svelte`.
Zero imports, zero events — fully prop-controlled. Svelte 5 runes.

## Props API

| prop | type | default | meaning |
|---|---|---|---|
| `size` | `number` (px) | `96` | orb diameter; particles/glow/ring/count scale from it |
| `state` | `"idle" \| "working" \| "success" \| "error" \| "undo"` | `"idle"` | the engine state |
| `rate` | `number` (0..1) | `0.5` | working particle-ring angular velocity; `0` frozen, `1` fast |
| `count` | `number \| null` | `null` | optional number rendered under the orb (tabular-nums); `null` hides |
| `ariaLabel` | `string \| null` | `null` | overrides the computed zh aria-label (localize via i18n `orb.*` keys) |

No events emitted. Consumers change `state`; every transition is interruptible
mid-flight — a state change mid-animation re-times the running transition to
the new target (CSS transitions, never one-shot keyframes, except the eaten
particle which is a one-shot by design).

## State table

| state | visual | duration | easing | reduced-motion fallback |
|---|---|---|---|---|
| **idle** | slow breath — core scale 1 ↔ 1.02, JS interval flips the target every 4s; transition loop, interruptible | 4000ms each way (8000ms cycle) | ease-out `--ease` | static teal dot + glow (scale 1.0) |
| **working** | core compresses to 0.88; 8 particles orbit at radius `size×0.72`, ring angular velocity = `150×rate` °/s (0 frozen, 1 fast); 1px teal ripple ring pulses once per second | compress 220ms; rotation: 180°-tick linear transitions, `180/(150·rate)` ms per tick; ripple fade-in 150ms → expand to `size×1.15` + fade 600ms, 1s cycle | ease-out (compress/ripple); linear (rotation) | teal dot, opacity pulse 1 ↔ 0.55 (opacity only), 800ms each way |
| working · eaten particle | every ~2s the next particle gets eaten: scale 0.25 → 1 → shrinks to center with `blur(4px)` → 0, then respawns at the rim | 650ms one-shot (only keyframes in the component), respawn 750ms | `cubic-bezier(0.3, 0, 0.8, 1)` | none — particles hidden |
| **success** | single swell to 1.12, settle back to 1.0; one pulse ring expands teal → transparent | swell 220ms + settle 220ms; ring 300ms | ease-out `--ease` | teal dot + check-ish glow (static) |
| **error** | deflate to 0.92, turns amber (`#fbbf24`), flicker ×2 | deflate 220ms; flicker 1→0.4→1→0.4→1 at 120ms steps | ease-out | static amber dot |
| **undo** | gulp 1 → 0.94 → 1 + teal brightness flash; pulse ring plays BACKWARD — contracts from `size×1.15` into the orb | gulp in 150ms, out 120ms (exit 20% faster); flash 150ms; ring contract 300ms | ease-out (gulp); `cubic-bezier(0.4, 0, 1, 1)` ease-in (suction) | teal dot + static ring at `size×1.15` |

## Particle behavior

- **8 dots** (within the 6–10 PLAN band), 3–4px teal circles, per-particle
  opacity 0.55–0.90 (variance), evenly spaced 45° ± 5° jitter.
- Orbit radius `size × 0.72`; the whole ring rotates as one layer via linear
  CSS transitions in 180° ticks — `rate` maps to `150 × rate` °/s, so a rate
  change mid-flight re-times the in-flight rotation (interruptible).
- Every ~2s one particle (cyclical index) is eaten: one-shot keyframes
  `scale(0.25) → scale(1) → translateX(−radius) scale(0.05) + blur(4px) → opacity 0`,
  then pops back to the rim (respawn).

## Motion rules

- Standard 150–250ms, ease-out `cubic-bezier(0.2, 0, 0, 1)` (`--ease`); exits
  20% faster (undo gulp 150ms in / 120ms out).
- Nothing exceeds 300ms except the idle breath (4000ms each way) and the
  rate-driven ring rotation.
- Transforms are `scale` / `translate` / `opacity` / `filter` only — no layout
  properties. `will-change: transform, opacity` on particles only.
- Tokens from `app.css` (`--t-fast`, `--t-med`, `--ease`, `--mono`, `--mist`)
  with local fallbacks — the component compiles and runs standalone.

## DOM & accessibility

```
.orb-wrap (inline-flex column; --orb-size + glow/inset px vars)
├─ .orb[role="img"][aria-label][data-state]   ← the one sized div
│  ├─ .core (transform/opacity/filter transitioned) > .core-teal + .core-amber
│  ├─ .orbit > .p-wrap ×8 > .p-dot (eaten one-shot)
│  └─ .ripple (1px teal ring layer)
└─ .count (optional; tabular-nums, --mono, mist, size×0.16 clamp ≥12px)
```

- `aria-label` is computed per state in zh (idle/working/success/error/undo);
  consumers pass `ariaLabel={t("orb.<state>")}` to localize — keys exist in
  both zh and en dicts (`app/src/lib/i18n.ts`).
- Entire orb is `pointer-events: none` (decorative feedback; interactive
  controls live beside it).
- `prefers-reduced-motion` handled internally (matchMedia + CSS backstop):
  everything static, states shown as idle = solid teal dot, working =
  pulsing-opacity dot, success = teal + glow, error = amber dot, undo = dot
  with ring.
