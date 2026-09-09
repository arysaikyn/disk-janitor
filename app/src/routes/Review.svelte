<script lang="ts">
  import { onMount } from "svelte";
  import { t, gradeName, getLocale, onLocaleChange, evidenceName } from "../lib/i18n";
  import { invoke } from "../lib/ipc";

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
  let planSummary = $state<string | null>(null);

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

  function gradeColor(grade: string): string {
    switch (grade) {
      case "pure_cache":
        return "var(--grade-pure)";
      case "rebuildable_from_source":
        return "var(--grade-rebuild)";
      case "redownloadable":
        return "var(--grade-redl)";
      case "expensive":
        return "var(--grade-expensive)";
      default:
        return "var(--grade-hands)";
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
      const q = plan.totals;
      planSummary = t("plan.summary")
        .replace("{n}", String(q.quarantine_entries))
        .replace("{m}", String(q.prune_cmds))
        .replace("{k}", String(q.refused));
      onplan(plan.plan_id);
    } catch (e) {
      err = String(e);
    }
    building = false;
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
    <div class="rev-hero">
      <div class="rev-hero-inner">
        <div class="big num">{fmtBytes(heroBytes)}</div>
        <div class="meta">
          <div class="l1">
            {t("review.safe_count").replace("{n}", String(safeList.length))}
            <span class="sep">·</span>
            {t("review.judge_count").replace("{n}", String(judgeList.length))}
          </div>
          <div class="l2 num">0 {t("dash.destroyed")} · {t("plan.grace")}</div>
        </div>
      </div>
    </div>

    <!-- safe grades: ONE summary row, expandable (exception-centered review) -->
    <div
      class="safe-line"
      role="button"
      tabindex="0"
      onclick={() => (safeOpen = !safeOpen)}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          safeOpen = !safeOpen;
        }
      }}
    >
      <span class="dot" style="background: var(--grade-pure)"></span>
      <span class="lbl">
        <b class="num">{fmtBytes(safeBytes)}</b> {t("review.safe_lbl")} · {safeList.length}
        {t("common.candidates")}
      </span>
      <span class="chev {safeOpen ? 'open' : ''}">▾</span>
    </div>
    {#if safeOpen}
      <div class="safe-drawer">
        {#each safeList.slice(0, 5) as c (c.id)}
          <div class="sd-row">
            <span class="gdot" style="background: {gradeColor(c.grade)}"></span>
            <span class="sd-path">{c.path}</span>
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
      <div class="judge-list">
        {#each judgeList as c (c.id)}
          <div
            class="judge-row {openRow === c.id ? 'open' : ''}"
            role="button"
            tabindex="0"
            onclick={() => toggleRow(c.id)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                toggleRow(c.id);
              }
            }}
          >
            <span class="gdot" style="background: {gradeColor(c.grade)}"></span>
            <span class="grade-word">{gradeName(c.grade)}</span>
            <span class="jr-path">{c.path}</span>
            <span class="jr-bytes num">{fmtBytes(c.alloc_bytes)}</span>
            {#if c.grade === "hands_off"}
              <span class="jr-refuse">{refuseReason(c)}</span>
            {/if}
          </div>
          {#if openRow === c.id}
            <div class="jr-detail open">
              {#each c.evidence as ev, i (i)}
                <div class="ev">
                  <span class="n">{i + 1}</span>
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
            </div>
          {/if}
        {/each}
      </div>
    {/if}

    <div class="rev-foot">
      <span>{t("review.selected").replace("{n}", String(safeList.length))}</span>
      <span class="num">· {fmtBytes(safeBytes)}</span>
      {#if planSummary}<span class="plan-summary num">{planSummary}</span>{/if}
      <span class="spacer"></span>
      <button class="primary" disabled={building} onclick={build}>
        {t("review.build_plan")}
      </button>
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
    color: var(--text-faint);
  }

  .review {
    padding: 0 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  /* hero — one hero number per screen */
  .rev-hero {
    border: 1px solid var(--hairline);
    border-radius: 14px;
    background: var(--bg-raised);
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.2);
  }
  .rev-hero-inner {
    display: flex;
    align-items: center;
    gap: 22px;
    padding: 18px 24px;
  }
  .rev-hero .big {
    font-size: 36px;
    font-weight: 700;
    letter-spacing: -2px;
  }
  .rev-hero .meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .rev-hero .l1 {
    font-size: 13px;
    font-weight: 600;
  }
  .rev-hero .l2 {
    font-size: 11px;
    color: var(--text-faint);
  }

  /* safe summary row + drawer */
  .safe-line {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border: 1px solid var(--hairline);
    border-radius: 10px;
    background: var(--bg-raised);
    cursor: pointer;
    font-size: 13px;
  }
  .safe-line:hover {
    background: #141c28;
  }
  .safe-line .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .safe-line .lbl {
    color: var(--text-dim);
  }
  .safe-line .lbl b {
    color: var(--text);
    font-weight: 600;
  }
  .safe-line .chev {
    margin-left: auto;
    color: var(--text-faint);
    font-size: 10px;
    transition: transform 0.2s cubic-bezier(0.32, 0.72, 0, 1);
  }
  .safe-line .chev.open {
    transform: rotate(180deg);
  }
  .safe-drawer {
    border: 1px solid var(--hairline);
    border-top: none;
    border-radius: 0 0 10px 10px;
    background: var(--bg);
    padding: 4px 14px 8px;
    margin-top: -12px;
  }
  .sd-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 0;
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
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sd-bytes {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-faint);
  }
  .sd-more {
    padding: 6px 0 2px;
    font-size: 11px;
    color: var(--text-faint);
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
    color: var(--text-faint);
    font-weight: 400;
  }

  .judge-list {
    border: 1px solid var(--hairline);
    border-radius: 10px;
    overflow: hidden;
    background: var(--bg-raised);
  }
  .judge-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--hairline);
    cursor: pointer;
    font-size: 13px;
  }
  .judge-row:last-of-type {
    border-bottom: none;
  }
  .judge-row:hover,
  .judge-row.open {
    background: #141c28;
  }
  .gdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .grade-word {
    font-size: 11px;
    color: var(--text-dim);
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
  }
  .jr-bytes {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .jr-refuse {
    font-size: 11px;
    color: var(--grade-hands);
    flex-shrink: 0;
  }

  .jr-detail {
    display: none;
    padding: 8px 16px 12px 40px;
    background: #0a0f16;
    border-bottom: 1px solid var(--hairline);
  }
  .jr-detail.open {
    display: block;
  }
  .ev {
    display: flex;
    gap: 10px;
    font-size: 12px;
    padding: 3px 0;
    color: var(--text-dim);
  }
  .ev .n {
    color: var(--text-faint);
    font-size: 10px;
    width: 10px;
    flex-shrink: 0;
    padding-top: 1px;
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
    color: var(--text-faint);
    flex-shrink: 0;
    white-space: nowrap;
  }
  .regen {
    font-size: 11px;
    color: var(--text-faint);
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
    gap: 10px;
    padding-top: 2px;
    font-size: 12px;
    color: var(--text-faint);
  }
  .rev-foot b {
    color: var(--text);
  }
  .rev-foot .plan-summary {
    font-size: 11px;
    color: var(--accent);
  }
  .rev-foot .spacer {
    flex: 1;
  }

  .empty {
    text-align: center;
    padding: 70px 20px;
    color: var(--text-dim);
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
</style>
