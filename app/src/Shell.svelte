<script lang="ts">
  import Sidebar from "./lib/Sidebar.svelte";
  import Dashboard from "./routes/Dashboard.svelte";
  import Review from "./routes/Review.svelte";
  import Ledger from "./routes/Ledger.svelte";
  import Scan from "./routes/Scan.svelte";
  import PlanReview from "./routes/PlanReview.svelte";
  import Settings from "./lib/Settings.svelte";
  import PurgeGate from "./lib/PurgeGate.svelte";

  type Route = "dashboard" | "scan" | "review" | "plan" | "ledger";
  let route: Route = $state("dashboard");
  let settingsOpen = $state(false);
  let purgeGate = $state<{ planId: string; bytes: number } | null>(null);

  // Scan handoff: Scan view finishes → Review opens with the persisted result.
  let lastScanPath = $state<string | null>(null);

  function openScan() {
    route = "scan";
  }
  function scanDone(path: string) {
    lastScanPath = path;
    route = "review";
  }
  function openPlan(planId: string) {
    route = "plan";
  }
</script>

<div class="shell">
  <Sidebar
    active={route}
    onnavigate={(r) => (route = r as Route)}
    onsettings={() => (settingsOpen = true)}
  />
  <main>
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
    {/if}
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
  main {
    flex: 1;
    overflow-y: auto;
    padding-top: 28px; /* content under transparent titlebar */
  }
</style>
