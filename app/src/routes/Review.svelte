<script lang="ts">
  import { onMount } from "svelte";
  import { t, gradeName, getLocale, onLocaleChange, evidenceName } from "../lib/i18n";
  import { invoke } from "../lib/ipc";
  import MetricLine from "../lib/MetricLine.svelte";
  import Row from "../lib/Row.svelte";
  import PressButton from "../lib/PressButton.svelte";

  interface Props {
    scanPath: string | null;
    onplan: (planId: string) => void;
  }
  let { scanPath, onplan }: Props = $props();

  // Single-language mode (D-31): mirror the active locale so {#key locale}
  // re-renders this screen's strings when the titlebar switcher flips languages.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  interface Evidence {
    kind: string | Record<string, string>;
    detail: string;
    strength: string;
  }

  interface Candidate {
    id: string;
    path: string;
    alloc_bytes: number;
    app_bytes: number;
    grade: string;
    evidence: Evidence[];
    regen?: { command: string; cost_secs: number | null; network: boolean } | null;
    locked?: boolean;
  }

  interface LastScan {
    roots: string[];
    min_bytes: number;
    candidates: Candidate[];
  }

  interface PlanTotals {
    quarantine_bytes: number;
    quarantine_entries: number;
    prune_bytes: number;
    prune_cmds: number;
    refused: number;
    snapshot_caveat: string | null;
  }

  interface PlanManifest {
    plan_id: string;
    created_at: string;
    totals: PlanTotals;
  }

  // Exception-centered (SPEC §3.4): safe grades collapse to one summary row;
  // Expensive/HandsOff/orphan rows are the judgment surface.
  const SAFE = new Set(["pure_cache", "rebuildable_from_source", "redownloadable"]);

  let scan = $state<LastScan | null>(null);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let safeOpen = $state(false);
  let openRow = $state<string | null>(null);
  let building = $state(false);

  onMount(async () => {
    try {
      scan = (await invoke("get_last_scan", {})) as LastScan | null;
    } catch (e) {
      err = String(e);
    }
    loading = false;
  });

  const safeList = $derived(scan ? scan.candidates.filter((c) => SAFE.has(c.grade)) : []);
  const judgeList = $derived(scan ? scan.candidates.filter((c) => !SAFE.has(c.grade)) : []);
  const safeBytes = $derived(safeList.reduce((a, c) => a + c.alloc_bytes, 0));
  const heroBytes = $derived(
    scan ? scan.candidates.reduce((a, c) => a + c.alloc_bytes, 0) : 0,
  );

  // Row's grade prop takes the CSS var name minus the -- prefix (§3.1 dot map:
  // pure green, rebuild teal, redl blue, expensive amber, hands red).
  function gradeVar(grade: string): string {
    switch (grade) {
      case "pure_cache":
        return "grade-pure";
      case "rebuildable_from_source":
        return "grade-rebuild";
      case "redownloadable":
        return "grade-redl";
      case "expensive":
        return "grade-expensive";
      default:
        return "grade-hands";
    }
  }

  function evKind(kind: string | Record<string, string>): { key: string; label: string } {
    // FlagBlock serializes as an object {"flag_block": "..."}; everything else is a
    // plain kind string (snake_case serde or demo PascalCase — evidenceName handles both).
    const key = typeof kind === "string" ? kind : Object.keys(kind)[0] ?? "";
    return { key, label: evidenceName(key) };
  }

  function refuseReason(c: Candidate): string {
    const first = c.evidence[0];
    const key =
      typeof first?.kind === "string"
        ? first.kind
        : Object.keys(first?.kind ?? {})[0] ?? "other";
    // Normalize like evidenceName: serde snake_case (mtime_hot) and demo
    // PascalCase (MtimeHot) both resolve to the review.refuse_* dictionaries.
    const norm = key.toLowerCase().replace(/[_\s-]/g, "");
    const k = `review.refuse_${norm}`;
    const label = t(k);
    return label === k ? key : label;
  }

  function strengthLabel(s: string): string {
    // serde serializes weak/medium/strong; tolerate capitalized demo values.
    return t(`common.s_${s.toLowerCase()}`);
  }

  function toggleRow(cid: string) {
    openRow = openRow === cid ? null : cid;
  }

  async function build() {
    building = true;
    try {
      // scanPath prop is set after a fresh scan; the persisted path covers the
      // sidebar-entry flow (set by Scan.svelte when a scan completes).
      const path = scanPath ?? localStorage.getItem("last-scan-path");
      if (!path) throw new Error(t("review.no_scan"));
      const plan = (await invoke("build_plan", {
        scanJsonPath: path,
        policy: null,
        minBytes: null,
      })) as PlanManifest;
      localStorage.setItem("last-plan-id", plan.plan_id);
      onplan(plan.plan_id);
    } catch (e) {
      err = String(e);
    }
    building = false;
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
  <h1>{t("review.title")}</h1>
  {#if scan}
    <span class="sub num">
      {scan.candidates.length} {t("common.candidates")} · {fmtBytes(heroBytes)}
    </span>
  {/if}
</div>

<div class="review">
  {#if err}
    <div class="refusal">
      <span class="rdot"></span>
      <div>
        <div class="rt-title">{t("review.no_scan")}</div>
        <div class="rt-body">
          <span class="mono">{err}</span>
        </div>
      </div>
    </div>
  {:else if !loading && scan && scan.candidates.length === 0}
    <!-- empty state: done, nothing worth reclaiming — that is good news -->
    <div class="empty">
      <div class="big">{t("review.empty")}</div>
      <p>{t("review.empty2")}</p>
    </div>
  {:else if !loading && scan}
    <!-- top: the one number + the safe/judge split as mist text (no box) -->
    <MetricLine
      label={t("dash.reclaimable")}
      value={fmtBytes(heroBytes)}
      sub={`${t("review.safe_count").replace("{n}", String(safeList.length))} · ${t("review.judge_count").replace("{n}", String(judgeList.length))}`}
    />
    <div class="promise num">0 {t("dash.destroyed")} · {t("plan.grace")}</div>

    <!-- safe grades: ONE summary row, expandable (exception-centered review) -->
    <ul class="rows">
      <Row grade="grade-pure" index={0} onclick={() => (safeOpen = !safeOpen)}>
        <span class="row-main">
          <b class="num">{fmtBytes(safeBytes)}</b>
          <span class="lbl">{t("review.safe_lbl")} · {safeList.length} {t("common.candidates")}</span>
          <span class="chev" class:open={safeOpen}>▾</span>
        </span>
      </Row>
    </ul>
    {#if safeOpen}
      <div class="safe-drawer">
        {#each safeList.slice(0, 5) as c (c.id)}
          <div class="sd-row">
            <span class="sd-path mono">{c.path}</span>
            <span class="sd-bytes num">{fmtBytes(c.alloc_bytes)}</span>
          </div>
        {/each}
        {#if safeList.length > 5}
          <div class="sd-more">+{safeList.length - 5} {t("review.safe_more")}</div>
        {/if}
      </div>
    {/if}

    {#if judgeList.length > 0}
      <div class="judge-head">
        {t("review.judge_head")}
        <span class="n num">{judgeList.length}</span>
      </div>

      <!-- judgment rows + welded-visible refusals: red dot + reason (SPEC §5.2) -->
      <ul class="rows">
        {#each judgeList as c, i (c.id)}
          <Row grade={gradeVar(c.grade)} index={i + 1} onclick={() => toggleRow(c.id)}>
            <span class="row-main">
              <span class="grade-word">{gradeName(c.grade)}</span>
              <span class="jr-path mono">{c.path}</span>
              <span class="jr-bytes num">{fmtBytes(c.alloc_bytes)}</span>
              {#if c.grade === "hands_off"}
                <span class="jr-refuse">{refuseReason(c)}</span>
              {/if}
            </span>
          </Row>
          {#if openRow === c.id}
            <li class="jr-detail">
              {#each c.evidence as ev, k (k)}
                <div class="ev">
                  <span class="n">{k + 1}</span>
                  <span class="k">{evKind(ev.kind).label}</span>
                  <span class="v">{ev.detail}</span>
                  <span class="s">{strengthLabel(ev.strength)}</span>
                </div>
              {/each}
              {#if c.regen}
                <div class="regen">
                  {c.regen.command}
                  {#if c.regen.cost_secs != null}
                    · {t("review.regen_approx")} {c.regen.cost_secs}s
                  {/if}
                  · {c.regen.network ? t("review.network") : t("review.no_network")}
                </div>
              {/if}
              {#if c.grade === "hands_off"}
                <div class="regen alt">{refuseReason(c)}</div>
              {/if}
            </li>
          {/if}
        {/each}
      </ul>
    {/if}

    <div class="rev-foot">
      <span class="spacer"></span>
      <PressButton kind="primary" label={t("review.build_plan")} disabled={building} onpress={build} />
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
  .sub {
    font-size: 12px;
    color: var(--mist);
  }

  .review {
    padding: 0 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    flex: 1;
    min-height: 0;
  }

  .promise {
    font-size: 12px;
    color: var(--mist);
  }

  /* row lists — hairline separators only, no boxes (Row ships its own
     per-row top hairline; first row exempted via :first-child) */
  .rows {
    display: flex;
    flex-direction: column;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row-main {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }
  .row-main b {
    color: var(--text);
    font-weight: 600;
  }
  .row-main .lbl {
    color: var(--mist);
  }
  .chev {
    margin-left: auto;
    color: var(--mist);
    font-size: 10px;
    transition: transform var(--t-med) var(--ease);
  }
  .chev.open {
    transform: rotate(180deg);
  }

  /* safe drawer: indented rows, hairline separators */
  .safe-drawer {
    padding: 2px 2px 6px;
  }
  .sd-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 2px;
    border-bottom: 1px solid var(--hairline);
    font-size: 12px;
  }
  .sd-row:last-child {
    border-bottom: none;
  }
  .sd-path {
    flex: 1;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--mist);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sd-bytes {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--mist);
  }
  .sd-more {
    padding: 6px 2px 2px;
    font-size: 11px;
    color: var(--mist);
  }

  .judge-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-size: 13px;
    font-weight: 600;
    padding: 4px 2px 0;
  }
  .judge-head .n {
    font-size: 11px;
    color: var(--mist);
    font-weight: 400;
  }

  .grade-word {
    font-size: 11px;
    color: var(--mist);
    width: 84px;
    flex-shrink: 0;
  }
  .jr-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text);
  }
  .jr-bytes {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--mist);
    flex-shrink: 0;
  }
  .jr-refuse {
    font-size: 11px;
    color: var(--grade-hands);
    flex-shrink: 0;
  }

  /* evidence detail: attached to its row, hairline-topped, indented mist
     (no bottom border — the following Row brings its own top hairline) */
  .jr-detail {
    display: block;
    padding: 10px 16px 12px 34px;
    border-top: 1px solid var(--hairline);
    font-size: 12px;
    color: var(--mist);
  }
  .ev {
    display: flex;
    gap: 10px;
    padding: 3px 0;
  }
  .ev .n {
    color: var(--mist);
    font-size: 10px;
    width: 10px;
    flex-shrink: 0;
    padding-top: 1px;
    opacity: 0.7;
  }
  .ev .k {
    color: var(--text);
    flex-shrink: 0;
  }
  .ev .v {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 11px;
  }
  .ev .s {
    margin-left: auto;
    font-size: 10px;
    color: var(--mist);
    flex-shrink: 0;
    white-space: nowrap;
    opacity: 0.75;
  }
  .regen {
    font-size: 11px;
    color: var(--mist);
    margin-top: 6px;
    font-family: var(--mono);
  }
  .regen.alt {
    color: var(--grade-hands);
    border-top: 1px solid var(--hairline);
    padding-top: 6px;
  }

  .rev-foot {
    display: flex;
    align-items: center;
    padding-top: 4px;
  }
  .rev-foot .spacer {
    flex: 1;
  }

  .empty {
    text-align: center;
    padding: 70px 20px;
    color: var(--mist);
  }
  .empty .big {
    font-size: 15px;
    color: var(--text);
    font-weight: 600;
    margin-bottom: 4px;
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
</style>
