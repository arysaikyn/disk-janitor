<script lang="ts">
  import { onMount } from "svelte";
  import { t, getLocale, onLocaleChange } from "../lib/i18n";
  import { invoke, channel } from "../lib/ipc";
  import Orb from "../lib/Orb.svelte";
  import MetricLine from "../lib/MetricLine.svelte";
  import PressButton from "../lib/PressButton.svelte";

  interface Props {
    ondone: (scanPath: string) => void;
  }
  let { ondone }: Props = $props();

  // Single-language mode (D-31): mirror the active locale so {#key locale}
  // re-renders this screen's strings when the titlebar switcher flips languages.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  interface Evidence {
    kind: string;
    detail: string;
    strength: string;
  }

  type ScanMsg =
    | { t: "files"; files: number; bytes: number; rate_bps: number; current: string; total?: number }
    | { t: "candidate"; id: string; bytes: number; grade: string; evidence: Evidence[] }
    | { t: "root_error"; root: string; err: string }
    | { t: "finished"; files: number; bytes: number; elapsed_ms: number };

  type OrbState = "idle" | "working" | "success" | "error" | "undo";

  let orbState = $state<OrbState>("working");
  let files = $state(0);
  let bytes = $state(0);
  let rate = $state(0); // bytes/s
  let current = $state("");
  let candidates = $state(0);
  let candBytes = $state(0);
  let rootErrs = $state<{ root: string; err: string }[]>([]);
  let canceled = $state(false);
  let canceling = $state(false);
  let finished = $state(false);
  let errMsg = $state<string | null>(null);

  // Live rate normalized 0..1 for the orb: GiB/s clamped at 12 GiB/s
  // (0 freezes the particle field until real throughput arrives).
  const orbRate = $derived(Math.min(1, rate / 2 ** 30 / 12));

  // Rust sends Finished BEFORE start_scan resolves; the demo resolves early and
  // streams Finished later. Awaiting the event keeps the orb working until the
  // scan actually completes in both worlds.
  let resolveFinished: (() => void) | undefined;
  const finishedEvent = new Promise<void>((r) => (resolveFinished = r));

  onMount(async () => {
    const ch = channel<ScanMsg>();
    ch.onmessage = (msg: ScanMsg) => {
      switch (msg.t) {
        case "files": {
          files = msg.files;
          bytes = msg.bytes;
          rate = msg.rate_bps;
          current = msg.current;
          break;
        }
        case "candidate":
          candidates += 1;
          candBytes += msg.bytes;
          break;
        case "root_error":
          rootErrs = [...rootErrs, { root: msg.root, err: msg.err }];
          break;
        case "finished":
          finished = true;
          orbState = "success";
          resolveFinished?.();
          break;
      }
    };
    try {
      // Default root: home dir; tilde expansion is the host's concern (R1 demo path).
      const path = await invoke("start_scan", { roots: ["~"], minBytes: null, channel: ch });
      await Promise.race([finishedEvent, new Promise((r) => setTimeout(r, 60_000))]);
      localStorage.setItem("last-scan-path", String(path));
      if (finished) {
        // Let the success beat read before leaving the screen (900ms swell).
        await new Promise((r) => setTimeout(r, 900));
      }
      ondone(String(path));
    } catch (e) {
      errMsg = String(e);
    }
  });

  function cancel() {
    canceling = true;
    canceled = true;
    void invoke("cancel_scan", {});
  }

  function fmtGiB(bps: number): string {
    return (bps / 2 ** 30).toFixed(2);
  }

  function fmtBytes(n: number): string {
    if (!Number.isFinite(n) || n < 0) return "—";
    if (n === 0) return "0 GB";
    if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
    return `${(n / 1e3).toFixed(1)} KB`;
  }

  function fmtCount(n: number): string {
    return n.toLocaleString("en-US");
  }
</script>
{#key locale}


<div class="screen-head">
  <h1>{t("scan.title")}</h1>
</div>

{#if errMsg}
  <!-- P1 refusal shape: verdict + evidence + alternative -->
  <div class="refusal">
    <span class="rdot"></span>
    <div>
      <div class="rt-title">{t("scan.err_title")}</div>
      <div class="rt-body">
        <span class="mono">{errMsg}</span>
      </div>
      <div class="rt-alt">{t("scan.err_alt")}</div>
    </div>
  </div>
{:else}
  <div class="scan-stage">
    <!-- the orb IS the ring: particles spiral in at live rate, count = files -->
    <Orb size={220} state={orbState} rate={orbRate} count={files} />

    <div class="stats">
      <div class="stat-cell">
        <MetricLine label={t("scan.scanned")} value={fmtBytes(bytes)} sub={`${fmtCount(files)} ${t("unit.files")}`} />
      </div>
      <div class="stat-cell">
        <MetricLine label={t("scan.candidates")} value={fmtCount(candidates)} sub={fmtBytes(candBytes)} />
      </div>
      <div class="stat-cell">
        <MetricLine label={t("scan.rate")} value={`${fmtGiB(rate)} GiB${t("scan.rate_unit")}`} />
      </div>
    </div>

    {#if current}
      <div class="current mono">{current}</div>
    {/if}

    <div class="cancel-zone">
      <PressButton
        kind="ghost"
        label={t("scan.cancel")}
        disabled={canceling || finished}
        onpress={cancel}
      />
      <span class="note">{t("scan.cancel_note")}</span>
    </div>
  </div>

  {#if canceled}
    <!-- D-15: interrupt drops incomplete batches, keeps completed — no fake "complete" -->
    <div class="cancel-banner">
      <span class="rdot"></span>
      <span>{t("scan.cancel_banner")}</span>
    </div>
  {/if}

  <!-- root errors stream inline as refusal rows; scan continues (skip-and-continue) -->
  {#if rootErrs.length > 0}
    <div class="root-errs">
      {#each rootErrs as re (re.root + re.err)}
        <div class="root-row">
          <span class="rdot"></span>
          <span class="mono path">{re.root}</span>
          <span class="err mono">{re.err}</span>
        </div>
      {/each}
    </div>
  {/if}
{/if}


{/key}
<style>
  .screen-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 20px 28px 14px;
    flex-shrink: 0;
  }
  .screen-head h1 {
    font-size: 15px;
    font-weight: 600;
  }

  .scan-stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 22px;
    padding: 12px 40px 20px;
    min-height: 0;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    width: min(560px, 100%);
  }
  .stat-cell {
    padding: 4px 20px 6px;
    min-width: 0;
  }
  .stat-cell:first-child {
    padding-left: 2px;
  }
  .stat-cell:last-child {
    padding-right: 2px;
  }
  .stat-cell + .stat-cell {
    border-left: 1px solid var(--hairline);
  }

  .current {
    font-size: 11px;
    color: var(--mist);
    max-width: 420px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cancel-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .cancel-zone .note {
    font-size: 10px;
    color: var(--mist);
  }

  .cancel-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 28px 10px;
    padding: 10px 14px;
    border: 1px solid var(--hairline);
    border-left: 2px solid var(--grade-expensive);
    border-radius: var(--r-sm);
    background: var(--surface);
    font-size: 12px;
    color: var(--mist);
  }

  .root-errs {
    margin: 0 28px 18px;
    display: flex;
    flex-direction: column;
  }
  .root-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 11px;
    padding: 7px 2px;
    border-bottom: 1px solid var(--hairline);
  }
  .root-row:last-child {
    border-bottom: none;
  }
  .root-row .path {
    color: var(--mist);
  }
  .root-row .err {
    color: var(--grade-hands);
    margin-left: auto;
  }

  .rdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--grade-hands);
    flex-shrink: 0;
  }

  /* P1 refusal banner */
  .refusal {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    margin: 0 28px;
    border: 1px solid var(--hairline);
    border-left: 2px solid var(--grade-hands);
    border-radius: var(--r-sm);
    padding: 12px 14px;
    background: var(--surface);
    font-size: 12px;
  }
  .rt-title {
    font-weight: 600;
    margin-bottom: 2px;
  }
  .rt-body {
    color: var(--mist);
    margin-bottom: 4px;
    word-break: break-all;
  }
  .mono {
    font-family: var(--mono);
  }
  .rt-alt {
    color: var(--mist);
  }
</style>
