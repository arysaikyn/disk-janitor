<script lang="ts">
  import { onMount } from "svelte";
  import { t, getLocale, onLocaleChange } from "../lib/i18n";
  import { invoke } from "../lib/ipc";
  import Orb from "../lib/Orb.svelte";
  import MetricLine from "../lib/MetricLine.svelte";
  import PressButton from "../lib/PressButton.svelte";

  interface Props {
    onstartscan: () => void;
    openPlan: (id: string) => void;
  }

  // Props contract with Shell: openPlan is the plan-handoff reserved for the
  // one-tap flow (D-24) — Dashboard cannot build a plan itself because
  // build_plan needs the scan-manifest path, which Shell owns after a scan.
  let { onstartscan }: Props = $props();

  // Single-language mode (D-31): mirror the active locale so {#key locale}
  // re-renders this screen's strings when the titlebar switcher flips languages.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  interface Volume {
    mount: string;
    fs: string;
    total: number;
    free_raw: number;
    read_only: boolean;
    writable: boolean;
    /** Not in the Rust VolumeDto yet; surfaced when the demo layer provides it. */
    snapshot_held?: number;
  }

  interface Candidate {
    id: string;
    path: string;
    alloc_bytes: number;
    grade: string;
  }

  interface LastScan {
    roots: string[];
    min_bytes: number;
    candidates: Candidate[];
  }

  // D-24: one-tap clean auto-marks these grades; anything else awaits confirmation.
  const AUTO = new Set(["pure_cache", "rebuildable_from_source"]);

  let volumes = $state<Volume[]>([]);
  let lastScan = $state<LastScan | null>(null);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let onetapNote = $state<string | null>(null);

  onMount(async () => {
    try {
      volumes = (await invoke("get_volumes", {})) as Volume[];
      lastScan = (await invoke("get_last_scan", {})) as LastScan | null;
    } catch (e) {
      err = String(e);
    }
    loading = false;
  });

  const vol = $derived(volumes[0] ?? null);
  const used = $derived(vol ? vol.total - vol.free_raw : 0);
  const recoverable = $derived(
    lastScan ? lastScan.candidates.reduce((a, c) => a + c.alloc_bytes, 0) : 0,
  );
  const awaiting = $derived(
    lastScan ? lastScan.candidates.filter((c) => !AUTO.has(c.grade)).length : 0,
  );
  // Snapshot footnote: hint always; the note only when a number is shown
  // (when snapshot_held is null the note IS the value — no em-dash dead end).
  const snapFootnote = $derived(
    vol?.snapshot_held ? `${t("dash.snap_hint")} · ${t("dash.snap_note")}` : t("dash.snap_hint"),
  );

  function onetap() {
    if (!lastScan) return;
    // D-24 stub: surfaces the explicit confirm list for anything outside
    // PureCache + RebuildableFromSource — never implies it saw everything.
    onetapNote = t("dash.onetap_pending").replace("{n}", String(awaiting));
  }

  function fmtBytes(n: number): string {
    if (!Number.isFinite(n) || n < 0) return "—";
    if (n === 0) return "0 GB";
    if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
    return `${(n / 1e3).toFixed(1)} KB`;
  }
</script>
{#key locale}


<div class="screen-head">
  <h1>{t("dash.title")}</h1>
</div>

<div class="dash">
  {#if err}
    <!-- P1 refusal shape: verdict + evidence + alternative (SPEC §5.2) -->
    <div class="refusal">
      <span class="rdot"></span>
      <div>
        <div class="rt-title">{t("dash.err_title")}</div>
        <div class="rt-body">
          <span class="mono">{err}</span>
        </div>
        <div class="rt-alt">{t("dash.err_alt")}</div>
      </div>
    </div>
  {/if}

  <!-- volume truth: one hairline row of three metric groups (SPEC §3.1) -->
  <div class="metric-row">
    <div class="metric-cell">
      <MetricLine
        label={t("dash.used")}
        value={vol ? fmtBytes(used) : "—"}
        sub={vol ? `/ ${fmtBytes(vol.total)}` : undefined}
      />
    </div>
    <div class="metric-cell">
      <MetricLine label={t("dash.free")} value={vol ? fmtBytes(vol.free_raw) : "—"} />
    </div>
    <div class="metric-cell">
      {#if vol?.snapshot_held}
        <MetricLine label={t("dash.snapshot")} value={fmtBytes(vol.snapshot_held)} sub={t("dash.snap_note")} />
      {:else}
        <MetricLine label={t("dash.snapshot")} value={t("dash.snap_hint")} small />
      {/if}
    </div>
  </div>

  <!-- the engine at rest beside the one number that matters -->
  <div class="orb-row">
    <Orb size={96} state="idle" />
    <MetricLine label={t("dash.reclaimable")} value={fmtBytes(recoverable)} sub={t("dash.undo_promise")} />
  </div>

  <div class="actions">
    <PressButton kind="primary" label={t("dash.scan_btn")} onpress={() => onstartscan()} />
    <PressButton kind="ghost" label={t("dash.onetap_btn")} disabled={!lastScan} onpress={onetap} />
    {#if onetapNote}<span class="note">{onetapNote}</span>{/if}
  </div>

  <!-- snapshot footnote: plain mist text, no box -->
  <div class="footnote">{snapFootnote}</div>

  {#if lastScan}
    <div class="last-scan hairline-b">
      <span class="lbl">{t("dash.last_scan")}</span>
      <span class="num val">{lastScan.candidates.length} {t("common.candidates")}</span>
      <span class="dim mono">{lastScan.roots[0] ?? ""}</span>
    </div>
  {/if}
</div>


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
  .dash {
    padding: 0 28px 28px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* three labeled numbers — hairline-separated, not cards */
  .metric-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
  }
  .metric-cell {
    padding: 4px 20px 6px;
    min-width: 0;
  }
  .metric-cell:first-child {
    padding-left: 2px;
  }
  .metric-cell:last-child {
    padding-right: 2px;
  }
  .metric-cell + .metric-cell {
    border-left: 1px solid var(--hairline);
  }

  /* the orb at rest + the reclaimable number */
  .orb-row {
    display: flex;
    align-items: center;
    gap: 28px;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .actions .note {
    font-size: 11px;
    color: var(--mist);
  }

  .footnote {
    font-size: 11px;
    color: var(--mist);
  }

  .last-scan {
    display: flex;
    align-items: baseline;
    gap: 14px;
    padding: 2px 2px 12px;
    font-size: 12px;
  }
  .last-scan .lbl {
    color: var(--mist);
  }
  .last-scan .val {
    color: var(--text);
  }
  .last-scan .dim {
    color: var(--mist);
    font-size: 11px;
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 40%;
  }

  /* P1 refusal banner */
  .refusal {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    border: 1px solid var(--hairline);
    border-left: 2px solid var(--grade-hands);
    border-radius: var(--r-sm);
    padding: 12px 14px;
    background: var(--surface);
    font-size: 12px;
  }
  .rdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--grade-hands);
    flex-shrink: 0;
    margin-top: 4px;
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
