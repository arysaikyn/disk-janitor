<script lang="ts">
  import { onMount } from "svelte";
  import { t, getLocale, onLocaleChange } from "../lib/i18n";
  import { invoke, channel } from "../lib/ipc";

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

  const pruneCmds = $derived(
    plan
      ? plan.entries
          .filter((e) => e.action.type === "prune_command")
          .map((e) => (e.action as { cmd: string }).cmd)
      : [],
  );
  // Plain function: called from the template, so reactive reads of `plan` are
  // tracked at render time (Svelte 5).
  function refuseByReason(): string[] {
    const counts = new Map<string, number>();
    for (const e of plan?.entries ?? []) {
      if (e.action.type !== "refuse") continue;
      const reason = (e.action as { reason: string }).reason;
      counts.set(reason, (counts.get(reason) ?? 0) + 1);
    }
    return [...counts.entries()].map(([reason, n]) => `${n} × ${reason}`);
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
      if (rows.length > 50) rows = rows.slice(-50);
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
    <button onclick={() => (jsonOpen = !jsonOpen)}>
      {jsonOpen ? t("plan.hide_json") : t("plan.view_json")}
    </button>
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
    <div class="plan-card">
      <div class="plan-row">
        <span class="qty num">{plan.totals.quarantine_entries} ×</span>
        <span class="what">{t("plan.quarantine")}</span>
        <span class="size num">{fmtBytes(plan.totals.quarantine_bytes)}</span>
        <span class="note num">{t("plan.freed_zero")}</span>
      </div>
      {#if plan.totals.prune_cmds > 0}
        <div class="plan-row">
          <span class="qty num">{plan.totals.prune_cmds} ×</span>
          <span class="what">
            {t("plan.prune")}
            <span class="cmd">{pruneCmds.join(" · ")}</span>
          </span>
          <span class="size num">{fmtBytes(plan.totals.prune_bytes)}</span>
        </div>
      {/if}
      {#if plan.totals.refused > 0}
        <div class="plan-row">
          <span class="qty num">{plan.totals.refused} ×</span>
          <span class="what">{t("plan.refused")}</span>
          <span class="size num">—</span>
          <span class="note mono">{refuseByReason().join(" · ")}</span>
        </div>
      {/if}
      <div class="plan-total">
        <span class="t num">
          {fmtBytes(plan.totals.quarantine_bytes + plan.totals.prune_bytes)}
          <span class="dedup">{t("plan.dedup")}</span>
        </span>
        <span class="grace">{t("plan.grace")}</span>
        {#if plan.totals.snapshot_caveat}
          <span class="snap">{t("plan.snapshot_note")}: {plan.totals.snapshot_caveat}</span>
        {/if}
      </div>
    </div>

    {#if applied}
      <div class="applied-note num">
        {t("plan.applied_done")} {fmtBytes(applied.quarantined_bytes)} · {t("plan.freed_zero")}
      </div>
    {/if}

    {#if rows.length > 0}
      <div class="act-list">
        <div class="act-head">
          <span>{t("plan.actions")}</span>
          <span class="spacer"></span>
          <!-- the reversibility claim, made numeric: apply frees 0 bytes (SPEC §4.3) -->
          <span class="freed num">{t("plan.freed_zero")}</span>
        </div>
        {#each rows as row (row.key)}
          <div class="act-row {row.kind}">
            <span class="mark">{row.kind === "ok" ? "✓" : "✕"}</span>
            <span class="path">{row.text}</span>
          </div>
        {/each}
      </div>
    {/if}

    {#if jsonOpen}
      <pre class="json-view">{JSON.stringify(plan, null, 2)}</pre>
    {/if}

    <div class="plan-foot">
      <button class="primary" disabled={applying || !planId} onclick={apply}>
        {t("plan.apply")}
      </button>
      <span class="hint-note">{t("plan.apply_hint")}</span>
      <span class="spacer"></span>
      <!-- purge only ever appears behind an applied quarantine (SPEC §3.5/§4.2) -->
      <button disabled={!applied || applying} onclick={() => applied && onpurge(planId!, applied.quarantined_bytes)}>
        {t("plan.purge")}
      </button>
      <span class="hint-note">{t("plan.purge_hint")}</span>
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
  .spacer {
    flex: 1;
  }

  .plan {
    padding: 0 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .plan-card {
    border: 1px solid var(--hairline);
    border-radius: 14px;
    background: var(--bg-raised);
    overflow: hidden;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.2);
  }
  .plan-row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 13px 18px;
    border-bottom: 1px solid var(--hairline);
  }
  .plan-row:last-child {
    border-bottom: none;
  }
  .plan-row .qty {
    font-family: var(--mono);
    font-size: 13px;
    font-weight: 600;
    width: 72px;
    flex-shrink: 0;
  }
  .plan-row .what {
    flex: 1;
    font-size: 13px;
  }
  .plan-row .what .cmd {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-faint);
  }
  .plan-row .size {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .plan-row .note {
    font-size: 11px;
    color: var(--text-faint);
    flex-shrink: 0;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .plan-total {
    display: flex;
    align-items: baseline;
    gap: 20px;
    padding: 13px 18px;
    background: #0a0f16;
  }
  .plan-total .t {
    font-size: 15px;
    font-weight: 650;
  }
  .plan-total .dedup {
    font-size: 11px;
    color: var(--text-faint);
    margin-left: 6px;
  }
  .plan-total .grace {
    font-size: 12px;
    color: var(--text-faint);
  }
  .plan-total .snap {
    font-size: 11px;
    color: var(--grade-expensive);
    margin-left: auto;
    max-width: 45%;
    text-align: right;
  }

  .applied-note {
    font-size: 12px;
    color: var(--accent);
  }

  /* live apply rows (ExecProgress stream) */
  .act-list {
    border: 1px solid var(--hairline);
    border-radius: 10px;
    background: var(--bg-raised);
    overflow: hidden;
  }
  .act-head {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 10px 16px;
    background: #0a0f16;
    border-bottom: 1px solid var(--hairline);
    font-size: 12px;
    color: var(--text-dim);
  }
  .act-head .spacer {
    flex: 1;
  }
  .act-head .freed {
    color: var(--accent);
    font-weight: 600;
  }
  .act-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    border-bottom: 1px solid var(--hairline);
    font-size: 12px;
  }
  .act-row:last-child {
    border-bottom: none;
  }
  .act-row .mark {
    flex-shrink: 0;
  }
  .act-row.ok .mark {
    color: var(--grade-pure);
  }
  .act-row.err .mark {
    color: var(--grade-hands);
  }
  .act-row .path {
    flex: 1;
    font-family: var(--mono);
    font-size: 11px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
  }

  .json-view {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--text-dim);
    background: #0a0f16;
    border: 1px solid var(--hairline);
    border-radius: 10px;
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
  }
  .hint-note {
    font-size: 11px;
    color: var(--text-faint);
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
