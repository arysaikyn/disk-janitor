<script lang="ts">
  // 还我磁盘 — 助手 Agent panel (SPEC Part C §3.9, simplified to a full screen
  // instead of the right dock). Propose-only by construction (D-01): the agent
  // returns plan ids + verdict cards; 「在计划中打开」 is the only CTA — approval
  // happens in Plan review, on the human's screen. Privacy: the AI only ever
  // sees opaque ids — paths stay in the app (ADR-7).
  import { onMount } from "svelte";
  import { t, getLocale, onLocaleChange, formatInt } from "../lib/i18n";
  import { invoke, channel } from "../lib/ipc";
  import type { AgentProgressMsg, AgentSummary } from "../lib/ipc";
  import Orb from "../lib/Orb.svelte";
  import MetricLine from "../lib/MetricLine.svelte";
  import PressButton from "../lib/PressButton.svelte";
  import EmptyState from "../lib/EmptyState.svelte";

  interface Props {
    /** Last completed scan JSON path (Shell owns it after a scan; falls back to
     *  localStorage, then to the backend default ~/last-scan.json). */
    scanPath?: string | null;
    onopenplan: (planId: string) => void;
    onopensettings: () => void;
  }
  let { scanPath = null, onopenplan, onopensettings }: Props = $props();

  // Single-language mode (D-31): mirror the active locale so {#key locale}
  // re-renders this screen's strings when the titlebar switcher flips languages.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  interface Candidate {
    id: string;
    alloc_bytes?: number;
    grade: string;
  }
  interface LastScan {
    roots: string[];
    min_bytes: number;
    candidates: Candidate[];
  }

  let lastScan = $state<LastScan | null>(null);
  let hasKey = $state(true); // pessimistic until get_model_config answers
  let prompt = $state("");
  let running = $state(false);
  let err = $state<string | null>(null);
  let summary = $state<AgentSummary | null>(null);
  let trajectory = $state<AgentProgressMsg[]>([]);
  let pulse = $state(0); // event pulses → orb rate
  let streamEl: HTMLDivElement | undefined = $state();
  let inputEl: HTMLTextAreaElement | undefined = $state();

  const PHASE_ICON: Record<string, string> = { model_call: "✻", tool_exec: "→", done: "✓" };

  onMount(async () => {
    try {
      lastScan = (await invoke("get_last_scan", {})) as LastScan | null;
    } catch (e) {
      err = String(e);
    }
    try {
      hasKey = Boolean((await invoke("get_model_config", {}))?.has_key);
    } catch {
      /* key unknown — keep the pessimistic default (no nag) */
    }
  });

  const reclaimable = $derived(
    lastScan ? lastScan.candidates.reduce((a, c) => a + (c.alloc_bytes ?? 0), 0) : 0,
  );
  const hasScan = $derived(!!lastScan && lastScan.candidates.length > 0);
  const lastEvent = $derived(trajectory[trajectory.length - 1] ?? null);
  const orbRate = $derived(Math.min(1, 0.35 + pulse * 0.13));

  function run() {
    if (running || !hasScan || !prompt.trim()) return;
    running = true;
    err = null;
    summary = null;
    trajectory = [];
    pulse = 0;
    const ch = channel<AgentProgressMsg>() as { onmessage?: (m: AgentProgressMsg) => void };
    ch.onmessage = (m: AgentProgressMsg) => {
      // Tolerate the demo channel's synthetic finished envelope (no phase).
      if (!m || typeof m.phase !== "string") return;
      trajectory = [...trajectory, m];
      pulse = Math.min(pulse + 1, 5);
      setTimeout(() => (pulse = Math.max(0, pulse - 1)), 420);
    };
    const path = scanPath ?? localStorage.getItem("last-scan-path") ?? "~/last-scan.json";
    invoke<AgentSummary>("run_agent", { scanJsonPath: path, prompt: prompt.trim(), channel: ch })
      .then((s) => {
        summary = s;
      })
      .catch((e) => {
        err = String(e);
      })
      .finally(() => {
        running = false;
      });
  }

  function cancel() {
    void invoke("cancel_agent", {});
  }

  function autosize() {
    const el = inputEl;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 108)}px`;
  }

  function onkeydown(e: KeyboardEvent) {
    // Enter sends — except during IME composition (CJK input confirms on Enter).
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      run();
    }
  }

  const stopWord = $derived(
    summary?.stop_reason === "user_cancel"
      ? t("agent.stop_cancel")
      : summary?.stop_reason === "requested_by_tool"
        ? t("agent.stop_tool")
        : (summary?.stop_reason ?? ""),
  );
  const budgetLine = $derived(
    summary
      ? t("agent.budget_line")
          .replace("{t}", String(summary.turns))
          .replace("{n}", formatInt(summary.tokens))
          .replace("{c}", fmtUsd(summary.cost_usd))
      : "",
  );
  const liveBudget = $derived(
    lastEvent
      ? `${t("agent.turn")} ${lastEvent.turn} · ${t("agent.tokens")} ${formatInt(lastEvent.budget.tokens)} · ${t("agent.cost")} ${fmtUsd(lastEvent.budget.cost_usd)}`
      : "",
  );

  function fmtBytes(n: number): string {
    if (!Number.isFinite(n) || n < 0) return "—";
    if (n === 0) return "0 GB";
    if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
    return `${(n / 1e3).toFixed(1)} KB`;
  }
  function fmtUsd(n: number): string {
    if (!Number.isFinite(n) || n < 0) return "—";
    return `$${n < 0.01 ? n.toFixed(4) : n.toFixed(2)}`;
  }

  // Live stream auto-scrolls to the newest row.
  $effect(() => {
    const n = trajectory.length; // tracked
    if (streamEl && n > 0) streamEl.scrollTop = streamEl.scrollHeight;
  });
</script>

{#snippet rows(events: AgentProgressMsg[])}
  {#each events as ev, i (i)}
    <div class="trow" style={`--stagger: ${Math.min(i, 8) * 40}ms`}>
      <span
        class="tic"
        class:tm={ev.phase === "model_call"}
        class:tt={ev.phase === "tool_exec"}
        class:td={ev.phase === "done"}
        aria-hidden="true"
      >{PHASE_ICON[ev.phase] ?? "·"}</span>
      <span class="turn num">{ev.turn}</span>
      {#if ev.tool}
        <span class="tool mono">{ev.tool}</span>
      {/if}
      <span class="detail">{ev.detail}</span>
    </div>
  {/each}
{/snippet}

{#key locale}
<div class="screen-head">
  <h1>{t("agent.title")}</h1>
</div>

<div class="agent">
  {#if err}
    <!-- P1 refusal shape: verdict + evidence + alternative (SPEC §5.2) -->
    <div class="refusal">
      <span class="rdot" aria-hidden="true"></span>
      <div>
        <div class="rt-title">{t("agent.unreachable")}</div>
        <div class="rt-body mono">{err}</div>
        <div class="rt-alt">{t("agent.error_hint")}</div>
      </div>
    </div>
  {/if}

  {#if !hasScan}
    <EmptyState title={t("agent.no_scan_title")} hint={t("agent.no_scan_hint")} />
  {:else}
    <div class="top">
      <MetricLine
        label={t("dash.reclaimable")}
        value={fmtBytes(reclaimable)}
        sub={t("agent.privacy")}
      />
    </div>

    {#if !hasKey}
      <div class="notice">
        <span class="notice-txt">{t("agent.no_key")}</span>
        <PressButton kind="ghost" label={t("agent.open_settings")} onpress={() => onopensettings()} />
      </div>
    {/if}

    {#if running}
      <div class="run-view">
        <div class="orb-wrap">
          <Orb size={120} state="working" rate={orbRate} ariaLabel={t("orb.working")} />
        </div>
        <div class="stream" bind:this={streamEl}>
          {@render rows(trajectory)}
        </div>
        <div class="run-foot">
          <span class="cost num">{liveBudget}</span>
          <PressButton kind="danger" label={t("common.cancel")} onpress={cancel} />
        </div>
      </div>
    {:else if summary}
      {#if summary.plan_id}
        <div class="card">
          <div class="card-head">
            <span class="card-title">{t("agent.proposal")}</span>
            <span class="card-plan mono">{summary.plan_id}</span>
          </div>
          {#if summary.proposal}
            <div class="card-body">{summary.proposal}</div>
          {/if}
          <div class="card-meta">
            <span class="verdict"><span class="v-lbl">{t("agent.verdict")}</span>{stopWord}</span>
            <span class="num">{budgetLine}</span>
          </div>
          <div class="card-actions">
            <PressButton
              kind="primary"
              label={t("agent.openinplan")}
              onpress={() => onopenplan(summary.plan_id as string)}
            />
          </div>
        </div>
      {:else}
        <div class="stop-note">
          <span class="stop-word">{stopWord}</span>
          <span class="num">{budgetLine}</span>
        </div>
      {/if}
      {#if trajectory.length > 0}
        <div class="stream history" bind:this={streamEl}>
          {@render rows(trajectory)}
        </div>
      {/if}
    {/if}

    {#if !running}
      <div class="input-row">
        <textarea
          bind:this={inputEl}
          bind:value={prompt}
          rows="1"
          placeholder={t("agent.placeholder")}
          spellcheck="false"
          oninput={autosize}
          onkeydown={onkeydown}
        ></textarea>
        <PressButton kind="primary" label={t("agent.think_btn")} disabled={!prompt.trim()} onpress={run} />
      </div>
    {/if}
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
  .agent {
    padding: 0 28px 28px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }

  .top {
    padding: 4px 2px 6px;
  }

  /* inline no-key notice — hairline bottom, no box */
  .notice {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 2px 10px;
    border-bottom: 1px solid var(--hairline);
  }
  .notice-txt {
    font-size: 12px;
    color: var(--mist);
    flex: 1;
  }

  /* running: orb + live trajectory + cancel */
  .run-view {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }
  .orb-wrap {
    padding-top: 6px;
  }
  .stream {
    width: 100%;
    max-height: 300px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .trow {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 7px 4px;
    font-size: 12.5px;
    border-top: 1px solid var(--hairline);
    transition: opacity var(--t-med) var(--ease) var(--stagger, 0ms);
  }
  .trow:first-child {
    border-top: none;
  }
  /* entry: fade in (rows appear instantly under reduced motion — tokens zero) */
  @starting-style {
    .trow {
      opacity: 0;
    }
  }
  .tic {
    width: 16px;
    flex: none;
    text-align: center;
    font-size: 12px;
  }
  .tm,
  .tt {
    color: var(--accent);
  }
  .td {
    color: var(--mist);
  }
  .turn {
    width: 14px;
    flex: none;
    text-align: right;
    font-size: 11px;
    color: var(--mist);
  }
  .tool {
    font-size: 11px;
    color: var(--text);
  }
  .detail {
    color: var(--mist);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .run-foot {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .cost {
    font-size: 12px;
    color: var(--mist);
  }

  /* proposal card — surface elevation, the one box of the screen */
  .card {
    background: var(--surface);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-1);
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .card-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--mist);
  }
  .card-plan {
    font-size: 12px;
    color: var(--text);
  }
  .card-body {
    font-size: 14px;
    line-height: 1.6;
    color: var(--text);
  }
  .card-meta {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    font-size: 12px;
    color: var(--mist);
  }
  .verdict {
    color: var(--accent);
  }
  .v-lbl {
    color: var(--mist);
    margin-right: 8px;
  }
  .card-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 2px;
  }

  /* terminal line when the run stopped without a plan (e.g. user cancel) */
  .stop-note {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 2px;
    font-size: 12px;
    color: var(--mist);
    border-top: 1px solid var(--hairline);
  }
  .stop-word {
    color: var(--text);
  }

  .history {
    max-height: 320px;
  }

  /* auto-growing prompt — 1–4 rows (40–108px) */
  .input-row {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    margin-top: 2px;
  }
  .input-row textarea {
    flex: 1;
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 0;
    border-radius: var(--r-sm);
    box-shadow: inset 0 0 0 1px var(--hairline);
    min-height: 40px;
    max-height: 108px;
    padding: 10px 12px;
    line-height: 1.5;
    resize: none;
    overflow-y: auto;
  }
  .input-row textarea::placeholder {
    color: var(--text-faint);
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

  @media (prefers-reduced-motion: reduce) {
    .trow {
      transition-delay: 0ms !important;
    }
  }
</style>
