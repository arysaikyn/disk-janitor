<script lang="ts">
  import { onMount } from "svelte";
  import { t, getLocale, onLocaleChange } from "../lib/i18n";
  import { invoke } from "../lib/ipc";

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
  const usedPct = $derived(vol && vol.total > 0 ? (used / vol.total) * 100 : 0);
  const recoverable = $derived(
    lastScan ? lastScan.candidates.reduce((a, c) => a + c.alloc_bytes, 0) : 0,
  );
  const awaiting = $derived(
    lastScan ? lastScan.candidates.filter((c) => !AUTO.has(c.grade)).length : 0,
  );

  function onetap() {
    if (!lastScan) return;
    // D-24 stub: surfaces the explicit confirm list for anything outside
    // PureCache + RebuildableFromSource — never implies it saw everything.
    onetapNote = t("dash.onetap_pending").replace("{n}", String(awaiting));
  }

  function fmtBytes(n: number): string {
    if (!Number.isFinite(n) || n < 0) return "—";
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

  {#if vol}
    <div class="vol-block">
      <div class="vol-name">
        {vol.mount}
        {#if vol.fs}<span class="tag">{vol.fs}</span>{/if}
        {#if vol.writable}<span class="tag ok">{t("dash.writable")}</span>{/if}
      </div>
      <div class="vol-bar"><div style="width: {usedPct}%"></div></div>
      <div class="vol-free num">{fmtBytes(vol.free_raw)} <span>{t("common.available")}</span></div>
    </div>
  {/if}

  {#if !loading && !lastScan}
    <!-- first-run empty state (D-05): hint line, no wizard -->
    <div class="empty">
      <div class="big">{t("dash.empty_title")}</div>
      <p>{t("dash.empty_privacy")}</p>
      <button class="primary" onclick={onstartscan}>{t("dash.start_first")}</button>
    </div>
  {:else}
    <div class="hero">
      <div class="hero-inner">
        <div class="hero-nums">
          <div class="big num">
            {fmtBytes(recoverable)}<span class="unit"> {t("dash.reclaimable")}</span>
          </div>
          <div class="row2">
            <span class="num ok">0 {t("dash.destroyed")}</span>
            <span class="num">{lastScan ? lastScan.candidates.length : 0} {t("common.candidates")}</span>
            <span class="num dim">{t("dash.until_confirm")}</span>
          </div>
        </div>
        <div class="hero-actions">
          <button class="primary" onclick={() => onstartscan()}>{t("dash.scan_btn")}</button>
          <button disabled={!lastScan} onclick={onetap}>{t("dash.onetap_btn")}</button>
          {#if onetapNote}<div class="onetap-note">{onetapNote}</div>{/if}
        </div>
      </div>
    </div>

    {#if lastScan}
      <div class="last-scan hairline-b">
        <span class="lbl">{t("dash.last_scan")}</span>
        <span class="num val">{lastScan.candidates.length} {t("common.candidates")}</span>
        <span class="dim mono">{lastScan.roots[0] ?? ""}</span>
      </div>
    {/if}
  {/if}

  <!-- volume truth: raw free + snapshot-held as two labeled numbers (SPEC §3.1) -->
  <div class="row3">
    <div>
      <div class="lbl">{t("dash.used")}</div>
      <div class="val num">{fmtBytes(used)}</div>
      <div class="note num">/ {vol ? fmtBytes(vol.total) : "—"}</div>
    </div>
    <div>
      <div class="lbl">{t("dash.free")}</div>
      <div class="val num">{vol ? fmtBytes(vol.free_raw) : "—"}</div>
    </div>
    <div>
      <div class="lbl">
        {t("dash.snapshot")} <span class="hint">{t("dash.snap_hint")}</span>
      </div>
      <div class="val num">{vol ? (vol.snapshot_held ? fmtBytes(vol.snapshot_held) : "—") : "—"}</div>
    </div>
  </div>
  <div class="footnote">
    {t("dash.snap_hint")} {t("dash.snap_note")}
  </div>
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
    gap: 18px;
  }

  /* volume truth block */
  .vol-block {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  .vol-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim);
    white-space: nowrap;
  }
  .vol-name .tag {
    font-size: 10px;
    color: var(--accent);
    border: 1px solid var(--hairline);
    border-radius: 4px;
    padding: 1px 6px;
    margin-left: 6px;
    font-weight: 500;
  }
  .vol-name .tag.ok {
    color: var(--text-dim);
  }
  .vol-bar {
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--bg-raised);
    overflow: hidden;
  }
  .vol-bar > div {
    height: 100%;
    background: linear-gradient(90deg, var(--hairline), var(--accent));
    border-radius: 3px;
  }
  .vol-free {
    font-size: 12px;
    color: var(--text-faint);
    white-space: nowrap;
  }

  /* P1 refusal banner */
  .refusal {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    border: 1px solid var(--hairline);
    border-left: 2px solid var(--grade-hands);
    border-radius: 8px;
    padding: 12px 14px;
    background: var(--bg-raised);
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
    color: var(--text-dim);
    margin-bottom: 4px;
    word-break: break-all;
  }
  .mono {
    font-family: var(--mono);
  }
  .rt-alt {
    color: var(--text-faint);
  }

  /* hero stage — one hero number per screen */
  .hero {
    border: 1px solid var(--hairline);
    border-radius: 14px;
    background: var(--bg-raised);
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.2);
  }
  .hero-inner {
    display: flex;
    align-items: center;
    gap: 28px;
    padding: 22px 26px;
  }
  .hero-nums {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hero-nums .big {
    font-size: 32px;
    font-weight: 700;
    letter-spacing: -1.5px;
  }
  .hero-nums .big .unit {
    font-size: 14px;
    color: var(--text-faint);
    font-weight: 500;
    letter-spacing: 0;
  }
  .row2 {
    display: flex;
    gap: 18px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .row2 .ok {
    color: var(--accent);
  }
  .row2 .dim {
    color: var(--text-faint);
  }
  .hero-actions {
    margin-left: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: stretch;
  }
  .onetap-note {
    font-size: 11px;
    color: var(--text-faint);
    text-align: center;
  }

  /* empty state */
  .empty {
    text-align: center;
    padding: 60px 20px 40px;
    color: var(--text-dim);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .empty .big {
    font-size: 15px;
    color: var(--text);
    font-weight: 600;
  }
  .empty p {
    font-size: 12px;
    margin-bottom: 10px;
  }

  .last-scan {
    display: flex;
    align-items: baseline;
    gap: 14px;
    padding: 2px 2px 12px;
    font-size: 12px;
  }
  .last-scan .lbl {
    color: var(--text-faint);
  }
  .last-scan .val {
    color: var(--text-dim);
  }
  .last-scan .dim {
    color: var(--text-faint);
    font-size: 11px;
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 40%;
  }

  /* three labeled numbers — hairline-separated, not cards */
  .row3 {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 1px;
    background: var(--hairline);
    border-radius: 10px;
    overflow: hidden;
  }
  .row3 > div {
    background: var(--bg);
    padding: 13px 16px;
  }
  .row3 .lbl {
    font-size: 10px;
    color: var(--text-faint);
    margin-bottom: 3px;
  }
  .row3 .val {
    font-size: 17px;
    font-weight: 600;
  }
  .row3 .note {
    font-size: 10px;
    color: var(--text-faint);
    margin-top: 2px;
  }
  .hint {
    border-bottom: 1px dotted var(--hairline);
    cursor: help;
  }
  .footnote {
    font-size: 10px;
    color: var(--text-faint);
  }
</style>
