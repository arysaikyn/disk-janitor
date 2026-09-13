<script lang="ts">
  import { onMount } from "svelte";
  import { t, getLocale, onLocaleChange } from "../lib/i18n";
  import { invoke, channel } from "../lib/ipc";
  import MetricLine from "../lib/MetricLine.svelte";
  import Row from "../lib/Row.svelte";
  import PressButton from "../lib/PressButton.svelte";

  interface Props {
    onpurge: (planId: string, bytes: number) => void;
  }
  let { onpurge }: Props = $props();

  // Single-language mode (D-31): mirror the active locale so {#key locale}
  // re-renders this screen's strings when the titlebar switcher flips languages.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  interface Evidence {
    kind: string;
    detail: string;
    strength: string;
  }

  interface PlanEntry {
    cid: string;
    path: string;
    alloc_bytes: number;
    grade: string;
    action:
      | { type: "quarantine" }
      | { type: "prune_command"; cmd: string; dry_run: string }
      | { type: "refuse"; reason: string };
    evidence: Evidence[];
  }

  interface PlanManifest {
    plan_id: string;
    created_at: string;
    entries: PlanEntry[];
    totals: {
      quarantine_bytes: number;
      quarantine_entries: number;
      prune_bytes: number;
      prune_cmds: number;
      refused: number;
      snapshot_caveat: string | null;
    };
  }

  interface ApplyResult {
    op_id: string;
    applied: number;
    skipped: number;
    refused: number;
    quarantined_bytes: number;
  }

  type ExecMsg =
    | { t: "applied"; cid: string; to: string }
    | { t: "purged"; cid: string; bytes: number }
    | { t: "restored"; cid: string; to: string }
    | { t: "error"; cid: string; err: string };

  interface ExecRow {
    key: string;
    kind: "ok" | "err";
    text: string;
  }

  let plan = $state<PlanManifest | null>(null);
  let planId = $state<string | null>(null);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let applying = $state(false);
  let applied = $state<ApplyResult | null>(null);
  let rows = $state<ExecRow[]>([]);
  let jsonOpen = $state(false);

  onMount(async () => {
    // Plan id contract: Review.svelte writes "last-plan-id" when 生成计划 runs.
    // There is no get_plan command, so the manifest for display is re-built from
    // the persisted last scan (same candidates → same totals); apply/purge use
    // the stored plan id, which find_plan resolves by manifest filename.
    const stored = localStorage.getItem("last-plan-id");
    try {
      const path = localStorage.getItem("last-scan-path");
      if (!path) throw new Error(t("review.no_scan"));
      const fresh = (await invoke("build_plan", {
        scanJsonPath: path,
        policy: null,
        minBytes: null,
      })) as PlanManifest;
      plan = fresh;
      planId = stored ?? fresh.plan_id;
      if (!stored) localStorage.setItem("last-plan-id", fresh.plan_id);
    } catch (e) {
      err = String(e);
    }
    loading = false;
  });

  const displayId = $derived(
    planId && planId.startsWith("plan-") ? planId : planId ? `plan-${planId}` : null,
  );

  // Entries grouped by action — quarantine first, then prune commands, then
  // refusals; `i` is the global row index so the stagger flows down the page.
  type EntryKind = "quarantine" | "prune" | "refuse";
  interface DisplayEntry {
    e: PlanEntry;
    i: number;
    kind: EntryKind;
    cmd?: string;
    reason?: string;
  }
  function groupEntries(p: PlanManifest): DisplayEntry[] {
    const q: DisplayEntry[] = [];
    const pr: DisplayEntry[] = [];
    const r: DisplayEntry[] = [];
    for (const e of p.entries) {
      const a = e.action as { type: string; cmd?: string; reason?: string };
      if (a.type === "prune_command") pr.push({ e, kind: "prune", cmd: a.cmd });
      else if (a.type === "refuse") r.push({ e, kind: "refuse", reason: a.reason });
      else q.push({ e, kind: "quarantine" });
    }
    return [...q, ...pr, ...r].map((d, i) => ({ ...d, i }));
  }
  const displayEntries = $derived(plan ? groupEntries(plan) : []);

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

  // Refusal reason → localized label (same normalization as Review.refuseReason:
  // serde snake_case and demo PascalCase both resolve to review.refuse_*).
  function refuseLabel(reason: string): string {
    const norm = reason.toLowerCase().replace(/[_\s-]/g, "");
    const k = `review.refuse_${norm}`;
    const label = t(k);
    return label === k ? reason : label;
  }

  async function apply() {
    if (!planId) return;
    applying = true;
    rows = [];
    const ch = channel<ExecMsg>();
    ch.onmessage = (msg: ExecMsg) => {
      if (msg.t === "applied") {
        rows = [
          ...rows,
          { key: msg.cid, kind: "ok", text: `${msg.cid} · ${t("plan.quarantined")} → ${msg.to}` },
        ];
      } else if (msg.t === "error") {
        rows = [...rows, { key: msg.cid + msg.err, kind: "err", text: `${msg.cid} · ${msg.err}` }];
      }
      // Receipt is itemized and verbose by default (SPEC §9 row 54, borrowing the
      // S3 DeleteResult{Deleted[],Error[]} shape): every applied/refused entry keeps
      // its own line. Truncation here silently hid losses — the list scrolls instead.
    };
    try {
      const res = (await invoke("apply_plan", { planId, channel: ch })) as Partial<ApplyResult>;
      // Rust returns quarantined_bytes; the demo layer returns freed_bytes — both
      // are the applied quarantine total, so fall back across shapes.
      applied = {
        op_id: res.op_id ?? "",
        applied: res.applied ?? 0,
        skipped: res.skipped ?? 0,
        refused: res.refused ?? 0,
        quarantined_bytes: res.quarantined_bytes ?? plan?.totals.quarantine_bytes ?? 0,
      };
    } catch (e) {
      err = String(e);
    }
    applying = false;
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
  <h1>{t("plan.title")}</h1>
  {#if displayId}<span class="sub mono">{displayId}</span>{/if}
  <span class="spacer"></span>
  {#if plan}
    <PressButton
      kind="ghost"
      label={jsonOpen ? t("plan.hide_json") : t("plan.view_json")}
      onpress={() => (jsonOpen = !jsonOpen)}
    />
  {/if}
</div>

<div class="plan">
  {#if err}
    <div class="refusal">
      <span class="rdot"></span>
      <div>
        <div class="rt-title">{t("plan.err_title")}</div>
        <div class="rt-body">
          <span class="mono">{err}</span>
        </div>
      </div>
    </div>
  {:else if plan}
    <!-- manifest header: MetricLine group, no box (MetricLine draws no chrome) -->
    <div class="metrics">
      <MetricLine
        label={t("plan.quarantine")}
        value={fmtBytes(plan.totals.quarantine_bytes)}
        sub={`${plan.totals.quarantine_entries} ${t("unit.entries")}`}
      />
      {#if plan.totals.prune_cmds > 0}
        <MetricLine label={t("plan.prune")} value={fmtBytes(plan.totals.prune_bytes)} />
      {/if}
      {#if plan.totals.refused > 0}
        <MetricLine
          label={t("plan.refused")}
          value={String(plan.totals.refused)}
          sub={t("unit.entries")}
        />
      {/if}
    </div>
    <!-- the undo promise, made numeric (SPEC §4.3): nothing is destroyed here -->
    <div class="promise num">0 {t("dash.destroyed")} · {t("plan.grace")}</div>
    {#if plan.totals.snapshot_caveat}
      <div class="snap">{t("plan.snapshot_note")}: {plan.totals.snapshot_caveat}</div>
    {/if}

    {#if applied}
      <div class="applied-note num">
        {t("plan.applied_done")} {fmtBytes(applied.quarantined_bytes)} · {t("plan.freed_zero")}
      </div>
    {/if}

    <!-- entries: grouped by action — quarantine (grade dot), prune commands
         (mono), refusals (muted, red dot, localized reason) -->
    {#if displayEntries.length > 0}
      <ul class="rows">
        {#each displayEntries as d (d.e.cid)}
          {#if d.kind === "quarantine"}
            <Row grade={gradeVar(d.e.grade)} index={d.i}>
              <span class="row-main">
                <span class="e-path mono">{d.e.path}</span>
                <span class="e-bytes num">{fmtBytes(d.e.alloc_bytes)}</span>
              </span>
            </Row>
          {:else if d.kind === "prune"}
            <Row index={d.i}>
              <span class="row-main">
                <span class="e-cmd mono">{d.cmd}</span>
                <span class="e-bytes num">{fmtBytes(d.e.alloc_bytes)}</span>
              </span>
            </Row>
          {:else}
            <Row grade="grade-hands" index={d.i}>
              <span class="row-main muted">
                <span class="e-reason">{refuseLabel(d.reason ?? t("review.refuse_other"))}</span>
                <span class="e-path mono">{d.e.path}</span>
                <span class="e-bytes num">{fmtBytes(d.e.alloc_bytes)}</span>
              </span>
            </Row>
          {/if}
        {/each}
      </ul>
    {/if}

    {#if rows.length > 0}
      <div class="act-list">
        <div class="act-head">{t("plan.actions")}</div>
        <ul class="act-rows">
          {#each rows as row, i (row.key)}
            <li
              class="act-row {row.kind}"
              style={`--stagger: ${Math.min(i, 8) * 40}ms`}
            >
              <span class="mark">{row.kind === "ok" ? "✓" : "✕"}</span>
              <span class="path mono">{row.text}</span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if jsonOpen}
      <pre class="json-view">{JSON.stringify(plan, null, 2)}</pre>
    {/if}

    <div class="plan-foot">
      <PressButton
        kind="ghost"
        label={t("plan.apply")}
        disabled={applying || !planId}
        onpress={apply}
      />
      <span class="hint-note">{t("plan.apply_hint")}</span>
      <!-- purge only ever appears behind an applied quarantine (SPEC §3.5/§4.2) -->
      <PressButton
        kind="primary"
        label={t("plan.purge")}
        disabled={!applied || applying}
        onpress={() => applied && onpurge(planId!, applied.quarantined_bytes)}
      />
      <span class="hint-note">{t("plan.purge_hint")}</span>
      <span class="spacer"></span>
      <!-- the reversibility claim, made numeric: apply frees 0 bytes (SPEC §4.3) -->
      <span class="freed-num num">{t("plan.freed_zero")}</span>
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
  .spacer {
    flex: 1;
  }

  .plan {
    padding: 0 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    flex: 1;
    min-height: 0;
  }

  /* manifest header — metric group, no box; hierarchy from spacing alone */
  .metrics {
    display: flex;
    gap: 32px;
    align-items: baseline;
  }

  /* the undo promise, made numeric (same voice as Review's promise line) */
  .promise {
    font-size: 12px;
    color: var(--mist);
  }

  .snap {
    font-size: 11px;
    color: var(--grade-expensive);
  }

  .applied-note {
    font-size: 12px;
    color: var(--accent);
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
  .e-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--text);
  }
  .e-cmd {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--text);
  }
  .e-bytes {
    font-size: 12px;
    color: var(--mist);
    flex-shrink: 0;
  }
  .e-reason {
    font-size: 11px;
    color: var(--mist);
    flex-shrink: 0;
    max-width: 38%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* refusals: whole row muted — the red dot carries the refusal color */
  .row-main.muted .e-path,
  .row-main.muted .e-bytes {
    color: var(--mist);
  }

  /* live apply stream (ExecProgress) — unboxed, hairline-separated, staggered */
  .act-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .act-head {
    font-size: 13px;
    font-weight: 600;
    padding: 6px 2px 0;
  }
  .act-rows {
    display: flex;
    flex-direction: column;
    list-style: none;
    margin: 0;
    padding: 0;
    /* Verbose-by-default receipt: the full list is kept, so it must scroll
       rather than push the apply/purge controls off-screen. */
    max-height: 360px;
    overflow-y: auto;
  }
  .act-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 2px;
    border-top: 1px solid var(--hairline);
    font-size: 12px;
    transition: opacity var(--t-med) var(--ease) var(--stagger, 0ms);
  }
  .act-row:first-child {
    border-top: none;
  }
  /* entry: fade in (transform kept free) */
  @starting-style {
    .act-row {
      opacity: 0;
    }
  }
  .act-row .mark {
    flex-shrink: 0;
    font-size: 11px;
  }
  .act-row.ok .mark {
    color: var(--grade-pure);
  }
  .act-row.err .mark {
    color: var(--grade-hands);
  }
  .act-row .path {
    flex: 1;
    font-size: 11px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--mist);
  }

  .json-view {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--mist);
    background: var(--surface);
    border: 1px solid var(--hairline);
    border-radius: var(--r-md);
    padding: 12px 14px;
    max-height: 40vh;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .plan-foot {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-top: 4px;
  }
  .hint-note {
    font-size: 11px;
    color: var(--text-faint);
  }
  /* live freed counter — styled like a MetricLine sub, tabular-nums */
  .freed-num {
    font-size: 12px;
    color: var(--accent);
    font-weight: 600;
  }

  /* P1 refusal banner */
  .refusal {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    border: 1px solid var(--hairline);
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

  /* reduced motion: entry stagger collapses with the transition */
  @media (prefers-reduced-motion: reduce) {
    .act-row {
      transition-delay: 0ms !important;
    }
  }
</style>
