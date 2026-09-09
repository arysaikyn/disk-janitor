<!--
  还我磁盘 Giveback — 呼吸球 (the signature, docs/redesign-PLAN.md)

  One living sphere that IS the scan/clean/undo engine. Fully prop-controlled
  (no events). All state changes are CSS transitions — interruptible mid-flight;
  the ONLY keyframes are the one-shot eaten-particle animation. Nothing exceeds
  300ms except the idle breath (4s each way, 8s cycle) and the working ring
  rotation (rate-driven). prefers-reduced-motion is handled internally: every
  state collapses to a static dot (teal / pulsing-opacity / glow / amber / ring).

  Props
    size      px, default 96
    state     "idle" | "working" | "success" | "error" | "undo", default "idle"
    rate      0..1 — working particle angular velocity (0 frozen, 1 fast)
    count     optional number rendered under the orb (tabular-nums); null hides
    ariaLabel overrides the computed zh aria-label (localize via i18n orb.* keys)
-->
<script lang="ts">
  type OrbState = "idle" | "working" | "success" | "error" | "undo";

  let {
    size = 96,
    state: orbState = "idle",
    rate = 0.5,
    count = null,
    ariaLabel = null,
  }: {
    size?: number;
    state?: OrbState;
    rate?: number; // 0..1 — working angular velocity (0 = frozen, 1 = fast)
    count?: number | null;
    ariaLabel?: string | null;
  } = $props();

  // --- spec constants (docs/redesign-PLAN.md; motion tokens from app.css with
  //     local fallbacks so the component works standalone) ---
  const N_PARTICLES = 8; // 6–10 per PLAN
  const P_RADIUS_F = 0.72; // orbit radius = size × 0.72
  const P_SIZE = [3, 4]; // particle px (slight variance)
  const EAT_EVERY_MS = 2000; // a particle gets eaten every ~2s
  const EAT_DUR_MS = 650; // eaten one-shot animation length
  const OMEGA_MAX = 150; // deg/s of ring rotation at rate = 1
  const BREATH_MS = 4000; // idle breath: 4s each way (8s cycle)
  const BASE_TRANSITION =
    "transform var(--t-med, 220ms) var(--ease, cubic-bezier(0.2, 0, 0, 1)), " +
    "opacity var(--t-fast, 150ms) ease-out, filter 100ms ease-out";
  const EASE_OUT = "var(--ease, cubic-bezier(0.2, 0, 0, 1))";

  interface Particle {
    id: number;
    angle: number; // deg — evenly spaced + small jitter
    radius: number; // px
    half: number; // px — negative half-size for centering
    size: number; // px
    opacity: number; // 0.55–0.90 variance
    eaten: boolean;
  }

  // --- reactive motion state (CSS transitions driven by these) ---
  let reduced = $state(false);
  let scale = $state(1); // core state target (× breath in idle)
  let breath = $state(1); // idle breathing multiplier
  let coreOpacity = $state(1);
  let coreFlash = $state(1); // undo teal flash (brightness)
  let errLayer = $state(0); // amber layer opacity (error)
  let coreTransition = $state(BASE_TRANSITION);
  let glow = $state(false); // reduced-motion static glow
  let particles = $state<Particle[]>([]);
  let rotation = $state(0); // ring rotation (deg, grows monotonically)
  let orbitTransition = $state("none");
  let rScale = $state(1); // ripple ring scale
  let rOpacity = $state(0);
  let ringTransition = $state("none");

  const DEFAULT_LABELS: Record<OrbState, string> = {
    idle: "空闲 — 缓缓呼吸",
    working: "工作中 — 条目正被吸入",
    success: "完成 — 已释放空间",
    error: "出错 — 需要处理",
    undo: "还原中 — 删除正在撤回",
  };
  let label = $derived(ariaLabel ?? DEFAULT_LABELS[orbState]);

  // --- timer bookkeeping: everything is cancelled on state change / unmount ---
  let timers: ReturnType<typeof setTimeout>[] = [];
  let intervals: ReturnType<typeof setInterval>[] = [];
  let rotTimer: ReturnType<typeof setInterval> | null = null;

  function later(fn: () => void, ms: number): void {
    const t = setTimeout(() => {
      timers = timers.filter((x) => x !== t);
      fn();
    }, ms);
    timers.push(t);
  }
  function every(fn: () => void, ms: number): void {
    intervals.push(setInterval(fn, ms));
  }
  function clearAll(): void {
    for (const t of timers) clearTimeout(t);
    for (const t of intervals) clearInterval(t);
    if (rotTimer) {
      clearInterval(rotTimer);
      rotTimer = null;
    }
    timers = [];
    intervals = [];
  }

  // --- prefers-reduced-motion detection (static-state-dot mode) ---
  $effect(() => {
    if (typeof window === "undefined") return;
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    reduced = mq.matches;
    const onChange = (e: MediaQueryListEvent) => (reduced = e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  });

  function makeParticles(): void {
    const arr: Particle[] = [];
    for (let i = 0; i < N_PARTICLES; i++) {
      const px = P_SIZE[i % P_SIZE.length];
      arr.push({
        id: i,
        angle: (i * 360) / N_PARTICLES + ((i * 29) % 11) - 5,
        radius: size * P_RADIUS_F,
        half: -px / 2,
        size: px,
        opacity: 0.55 + ((i * 37) % 8) * 0.05,
        eaten: false,
      });
    }
    particles = arr;
  }

  /** Snap the ripple to (fromScale, fromOpacity), then transition to (toScale, toOpacity). */
  function playRing(
    fromScale: number,
    fromOpacity: number,
    toScale: number,
    toOpacity: number,
    ms: number,
    easing: string,
  ): void {
    ringTransition = "none";
    rScale = fromScale;
    rOpacity = fromOpacity;
    later(() => {
      ringTransition = `transform ${ms}ms ${easing}, opacity ${ms}ms ${easing}`;
      rScale = toScale;
      rOpacity = toOpacity;
    }, 30);
  }

  // --- the engine: one effect per state, everything interruptible ---
  $effect(() => {
    const s = orbState; // tracked
    const rm = reduced; // tracked
    clearAll();
    scale = 1;
    breath = 1;
    coreOpacity = 1;
    coreFlash = 1;
    errLayer = 0;
    glow = false;
    rotation = 0;
    particles = [];
    orbitTransition = "none";
    rScale = 1;
    rOpacity = 0;
    ringTransition = "none";
    coreTransition = rm ? "none" : BASE_TRANSITION;

    if (rm) {
      // everything static — states read as dots
      switch (s) {
        case "idle":
        case "success":
          glow = true; // idle: solid teal glow dot; success: teal + check-ish glow
          break;
        case "working":
          coreTransition = "opacity 800ms ease-out";
          every(() => {
            coreOpacity = coreOpacity === 1 ? 0.55 : 1;
          }, 800);
          break;
        case "error":
          errLayer = 1; // amber dot
          break;
        case "undo":
          rScale = 1.15; // dot with ring (static)
          rOpacity = 0.5;
          break;
      }
      return clearAll;
    }

    switch (s) {
      case "idle": {
        // slow breath — 1 ↔ 1.02, 4s each way, transition loop (interruptible)
        coreTransition = `transform ${BREATH_MS}ms ${EASE_OUT}`;
        every(() => {
          breath = breath === 1 ? 1.02 : 1;
        }, BREATH_MS);
        break;
      }
      case "working": {
        scale = 0.88; // compress
        makeParticles();
        // eat one particle every ~2s (cyclical), respawn at rim after the one-shot
        let idx = 0;
        every(() => {
          const p = particles[idx % particles.length];
          if (p && !p.eaten) {
            p.eaten = true;
            later(() => {
              p.eaten = false;
            }, EAT_DUR_MS + 100);
          }
          idx += 1;
        }, EAT_EVERY_MS);
        // ripple pulse: fade in 150ms → expand to size×1.15 + fade out 600ms, once per second
        every(() => {
          ringTransition = "none";
          rScale = 1;
          rOpacity = 0;
          later(() => {
            ringTransition = "opacity 150ms ease-out";
            rOpacity = 0.6;
            later(() => {
              ringTransition = "transform 600ms ease-out, opacity 600ms ease-out";
              rScale = 1.15;
              rOpacity = 0;
            }, 150);
          }, 30);
        }, 1000);
        break;
      }
      case "success": {
        scale = 1.12; // single swell, 220ms ease-out
        later(() => {
          scale = 1; // settle back
        }, 220);
        // one expanding pulse ring teal → transparent
        playRing(1, 0.6, 1.2, 0, 300, EASE_OUT);
        break;
      }
      case "error": {
        scale = 0.92; // deflate
        errLayer = 1; // amber
        // flicker ×2: 1 → 0.4 → 1 → 0.4 → 1, 150ms steps (matches --t-fast)
        later(() => {
          coreOpacity = 0.4;
        }, 0);
        later(() => {
          coreOpacity = 1;
        }, 150);
        later(() => {
          coreOpacity = 0.4;
        }, 300);
        later(() => {
          coreOpacity = 1;
        }, 450);
        break;
      }
      case "undo": {
        // gulp: 1 → 0.94 (150ms) → 1 (120ms — exit 20% faster) + teal flash
        coreTransition =
          "transform 150ms " + EASE_OUT + ", opacity 120ms ease-out, filter 100ms ease-out";
        scale = 0.94;
        coreFlash = 1.45;
        later(() => {
          coreTransition =
            "transform 120ms " + EASE_OUT + ", opacity 120ms ease-out, filter 100ms ease-out";
          scale = 1;
        }, 150);
        later(() => {
          coreFlash = 1;
        }, 150);
        // pulse ring plays BACKWARD: snap to size×1.15, contract into the orb
        ringTransition = "none";
        rScale = 1.15;
        rOpacity = 0.6;
        later(() => {
          ringTransition = "transform 300ms cubic-bezier(0.4, 0, 1, 1), opacity 300ms ease-in";
          rScale = 0.8;
          rOpacity = 0;
        }, 30);
        break;
      }
    }
    return clearAll;
  });

  // --- working ring rotation: angular velocity ∝ rate (0 frozen, 1 fast) ---
  $effect(() => {
    const s = orbState; // tracked
    const r = rate; // tracked
    const rm = reduced; // tracked
    if (rotTimer) {
      clearInterval(rotTimer);
      rotTimer = null;
    }
    if (s !== "working" || rm || r <= 0) return;
    const half = Math.max(120, Math.round((180 / (OMEGA_MAX * r)) * 1000)); // ms per 180°
    rotTimer = setInterval(() => {
      rotation += 180;
    }, half);
    orbitTransition = `transform ${half}ms linear`;
  });
</script>

<div
  class="orb-wrap"
  style="--orb-size: {size}px; --orb-glow: {Math.round(size * 0.22)}px; --orb-glow-strong: {Math.round(size * 0.34)}px; --orb-inset-y: {Math.round(size * -0.06)}px; --orb-inset-b: {Math.round(size * 0.16)}px"
>
  <div
    class="orb"
    role="img"
    aria-label={label}
    data-state={orbState}
    style="width: {size}px; height: {size}px"
  >
    <div
      class="core"
      class:glow={glow}
      style:transform={`scale(${scale * breath})`}
      style:opacity={coreOpacity}
      style:filter={`brightness(${coreFlash})`}
      style:transition={coreTransition}
    >
      <div class="core-teal"></div>
      <div class="core-amber" style:opacity={errLayer}></div>
    </div>

    {#if particles.length > 0}
      <div
        class="orbit"
        style:transform={`rotate(${rotation}deg)`}
        style:transition={orbitTransition}
      >
        {#each particles as p (p.id)}
          <span
            class="p-wrap"
            style="--p-angle: {p.angle}deg; --p-radius: {p.radius}px; --p-radius-n: {-p.radius}px"
          >
            <span
              class="p-dot"
              class:eaten={p.eaten}
              style="--p-half: {p.half}px; --p-size: {p.size}px; --p-opacity: {p.opacity}"
            ></span>
          </span>
        {/each}
      </div>
    {/if}

    <div
      class="ripple"
      style:transform={`scale(${rScale})`}
      style:opacity={rOpacity}
      style:transition={ringTransition}
    ></div>
  </div>

  {#if count !== null && count !== undefined}
    <div class="count" style="font-size: {Math.max(12, Math.round(size * 0.16))}px">
      {Number(count).toLocaleString()}
    </div>
  {/if}
</div>

<style>
  .orb-wrap {
    --orb-teal: #2dd4bf;
    --orb-teal-hi: #5eead4;
    --orb-teal-deep: #0e9f8c;
    --orb-amber: #fbbf24;
    --orb-amber-hi: #fde68a;
    --orb-amber-deep: #b45309;
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .orb {
    position: relative;
    pointer-events: none;
  }

  /* the sphere — teal layer + amber error layer, crossfaded */
  .core {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    box-shadow:
      0 0 var(--orb-glow) rgb(45 212 191 / 0.28),
      inset 0 var(--orb-inset-y) var(--orb-inset-b) rgb(4 20 18 / 0.5);
  }
  .core.glow {
    box-shadow:
      0 0 var(--orb-glow-strong) rgb(45 212 191 / 0.5),
      inset 0 var(--orb-inset-y) var(--orb-inset-b) rgb(4 20 18 / 0.5);
  }
  .core-teal,
  .core-amber {
    position: absolute;
    inset: 0;
    border-radius: 50%;
  }
  .core-teal {
    background: radial-gradient(
      circle at 32% 28%,
      var(--orb-teal-hi) 0%,
      var(--orb-teal) 46%,
      var(--orb-teal-deep) 100%
    );
  }
  .core-amber {
    background: radial-gradient(
      circle at 32% 28%,
      var(--orb-amber-hi) 0%,
      var(--orb-amber) 46%,
      var(--orb-amber-deep) 100%
    );
    transition: opacity var(--t-fast, 150ms) ease-out;
  }

  /* particle ring — dots ride a rotating layer, each with its own eaten one-shot */
  .orbit {
    position: absolute;
    inset: 0;
    z-index: 1;
  }
  .p-wrap {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 0;
    height: 0;
    transform: rotate(var(--p-angle)) translateX(var(--p-radius));
  }
  .p-dot {
    position: absolute;
    left: var(--p-half);
    top: var(--p-half);
    width: var(--p-size);
    height: var(--p-size);
    border-radius: 50%;
    background: var(--orb-teal);
    opacity: var(--p-opacity);
    will-change: transform, opacity;
  }
  .p-dot.eaten {
    animation: p-eaten 650ms cubic-bezier(0.3, 0, 0.8, 1) forwards;
  }
  /* the ONLY keyframes — a one-shot, so it may stay a keyframe */
  @keyframes p-eaten {
    0% {
      transform: scale(0.25);
      opacity: 1;
      filter: blur(0);
    }
    32% {
      transform: scale(1);
      opacity: 1;
      filter: blur(0);
    }
    100% {
      transform: translateX(var(--p-radius-n)) scale(0.05);
      opacity: 0;
      filter: blur(4px);
    }
  }

  /* ripple ring — 1px teal, pulses (working), exhales (success), contracts (undo) */
  .ripple {
    position: absolute;
    inset: 0;
    z-index: 2;
    border-radius: 50%;
    border: 1px solid var(--orb-teal);
  }

  /* count slot — tabular-nums, mist (numbers ARE the interface) */
  .count {
    font-family: var(--mono, "SF Mono", ui-monospace, monospace);
    font-variant-numeric: tabular-nums;
    line-height: 1;
    color: var(--mist, #8b98a8);
    letter-spacing: 0.02em;
  }

  /* backstop — JS already collapses to static dots; kill any stragglers */
  @media (prefers-reduced-motion: reduce) {
    .core,
    .core-amber,
    .orbit,
    .p-dot,
    .ripple {
      transition: none !important;
      animation: none !important;
    }
  }
</style>
