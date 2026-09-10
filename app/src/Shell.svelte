<script lang="ts">
  import Dashboard from "./routes/Dashboard.svelte";
  import Review from "./routes/Review.svelte";
  import Ledger from "./routes/Ledger.svelte";
  import Scan from "./routes/Scan.svelte";
  import PlanReview from "./routes/PlanReview.svelte";
  import AgentPanel from "./routes/AgentPanel.svelte";
  import Settings from "./lib/Settings.svelte";
  import PurgeGate from "./lib/PurgeGate.svelte";
  import { t, getLocale, onLocaleChange } from "./lib/i18n";

  type Route = "dashboard" | "scan" | "review" | "plan" | "ledger" | "agent";
  let route: Route = $state("dashboard");
  let settingsOpen = $state(false);
  let purgeGate = $state<{ planId: string; bytes: number } | null>(null);

  // Scan handoff: Scan view finishes → Review opens with the persisted result.
  let lastScanPath = $state<string | null>(null);

  // Single-language mode (D-31): mirror the active locale (switcher lives in
  // Settings) so nav strings re-render on switch.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  // Route change: content fades 150ms, opacity only, interruptible (CSS
  // transition on the wrapper — no keyframes). Rapid clicks collapse onto the
  // latest target; reduced-motion skips the delay entirely.
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  let fading = $state(false);
  let pending: Route | null = null;
  let fadeTimer: ReturnType<typeof setTimeout> | undefined;

  function goto(r: Route) {
    if (r === route && pending === null) return;
    if (reduced) {
      pending = null;
      route = r;
      fading = false;
      return;
    }
    pending = r;
    if (!fading) {
      fading = true;
      fadeTimer = setTimeout(() => {
        if (pending !== null) {
          route = pending;
          pending = null;
        }
        fading = false;
      }, 150);
    }
  }

  function openScan() {
    goto("scan");
  }
  function scanDone(path: string) {
    lastScanPath = path;
    goto("review");
  }
  function openPlan(_planId: string) {
    goto("plan");
  }
</script>

<div class="shell" lang={locale}>
  <nav class="sidebar">
    <button class="brand" onclick={() => goto("dashboard")} aria-label={t("nav.dash")}>
      <span class="brand-ic">↩</span>
      <span class="brand-name">{t("brand")}</span>
    </button>

    <div class="items">
      <button class="nav-btn" class:active={route === "dashboard"} onclick={() => goto("dashboard")}>
        <span class="dot" aria-hidden="true"></span>
        <span class="ic" aria-hidden="true">◈</span>
        <span>{t("nav.dash")}</span>
      </button>
      <button class="nav-btn" class:active={route === "review"} onclick={() => goto("review")}>
        <span class="dot" aria-hidden="true"></span>
        <span class="ic" aria-hidden="true">◉</span>
        <span>{t("nav.review")}</span>
      </button>
      <button class="nav-btn" class:active={route === "ledger"} onclick={() => goto("ledger")}>
        <span class="dot" aria-hidden="true"></span>
        <span class="ic" aria-hidden="true">≡</span>
        <span>{t("nav.ledger")}</span>
      </button>
      <button class="nav-btn" class:active={route === "agent"} onclick={() => goto("agent")}>
        <span class="dot" aria-hidden="true"></span>
        <span class="ic" aria-hidden="true">✦</span>
        <span>{t("nav.agent")}</span>
      </button>
      <button class="nav-btn" class:active={settingsOpen} onclick={() => (settingsOpen = true)}>
        <span class="dot" aria-hidden="true"></span>
        <span class="ic" aria-hidden="true">⚙</span>
        <span>{t("nav.settings")}</span>
      </button>
    </div>

    <div class="foot">
      <div>{t("sidebar.foot")}</div>
      <div class="foot2">{t("sidebar.foot2")}</div>
    </div>
  </nav>

  <main>
    <div class="screen-wrap" style="opacity: {fading ? 0 : 1}">
      {#if route === "dashboard"}
        <Dashboard onstartscan={openScan} {openPlan} />
      {:else if route === "scan"}
        <Scan ondone={scanDone} />
      {:else if route === "review"}
        <Review scanPath={lastScanPath} onplan={openPlan} />
      {:else if route === "plan"}
        <PlanReview onpurge={(id, bytes) => (purgeGate = { planId: id, bytes })} />
      {:else if route === "ledger"}
        <Ledger />
      {:else if route === "agent"}
        <AgentPanel scanPath={lastScanPath} onopenplan={openPlan} onopensettings={() => (settingsOpen = true)} />
      {/if}
    </div>
  </main>

  {#if settingsOpen}
    <Settings onclose={() => (settingsOpen = false)} />
  {/if}
  {#if purgeGate}
    <PurgeGate
      planId={purgeGate.planId}
      bytes={purgeGate.bytes}
      onclose={() => (purgeGate = null)}
    />
  {/if}
</div>

<style>
  .shell {
    display: flex;
    height: 100vh;
  }

  /* sidebar — nav items wear the PressButton-ghost treatment: hairline ring,
     hover lift, press scale; active route = teal text + dot indicator */
  .sidebar {
    width: 200px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--hairline);
    background: var(--ink);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 36px 18px 14px; /* top clears the transparent titlebar traffic lights */
    border: none;
    background: transparent;
    color: var(--text);
    font-size: 14px;
    font-weight: 600;
    text-align: left;
    letter-spacing: -0.1px;
    cursor: pointer;
  }
  .brand-ic {
    color: var(--accent);
    font-size: 17px;
    line-height: 1;
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 10px;
  }
  .nav-btn {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 0 12px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--mist);
    font-size: 13px;
    text-align: left;
    box-shadow: inset 0 0 0 1px var(--hairline);
    cursor: pointer;
    transition:
      color var(--t-fast) var(--ease),
      background var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
  }
  .nav-btn:hover {
    color: var(--text);
    background: var(--surface-lift);
    transform: translateY(-1px); /* hover lift */
  }
  .nav-btn:active {
    transform: translateY(0) scale(0.96);
  }
  .nav-btn.active {
    color: var(--accent);
  }
  .nav-btn .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease);
  }
  .nav-btn.active .dot {
    opacity: 1;
  }
  .nav-btn .ic {
    width: 16px;
    text-align: center;
    opacity: 0.85;
  }
  .foot {
    margin-top: auto;
    padding: 12px 18px 16px;
    font-size: 10px;
    color: var(--mist);
    line-height: 1.7;
  }

  main {
    flex: 1;
    overflow-y: auto;
    padding-top: 28px; /* content under transparent titlebar */
  }
  .screen-wrap {
    min-height: 100%;
    transition: opacity var(--t-fast) var(--ease);
  }

  @media (prefers-reduced-motion: reduce) {
    .nav-btn,
    .nav-btn .dot,
    .screen-wrap {
      transition: none;
    }
    .nav-btn:hover,
    .nav-btn:active {
      transform: none;
    }
  }
</style>
