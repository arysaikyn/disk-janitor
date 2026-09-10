<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { t, getLocale, onLocaleChange } from "../lib/i18n";
  import { invoke, channel } from "../lib/ipc";
  import EmptyState from "../lib/EmptyState.svelte";
  import PressButton from "../lib/PressButton.svelte";

  // Single-language mode (D-31): mirror the active locale so {#key locale}
  // re-renders this screen's strings when the titlebar switcher flips languages.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  type ExecMsg =
    | { t: "applied"; cid: string; to: string }
    | { t: "purged"; cid: string; bytes: number }
    | { t: "restored"; cid: string; to: string }
    | { t: "error"; cid: string; err: string };

  interface ApplyRec {
    t: "apply";
    plan_id: string;
    entry: unknown;
    original_path: string;
    quarantine_path: string;
    renamex_np: boolean;
    undo: { original_path: string; byte_count: number; grace_expires_at: string };
  }
  interface PurgeRec {
    t: "purge";
    plan_id: string;
    entry: unknown;
    bytes_freed: number;
    quarantine_path: string;
  }
  interface RestoreRec {
    t: "restore";
    plan_id: string;
    entry: unknown;
    from: string;
    to: string;
    undoes: number;
  }
  interface PlanRec {
    t: "plan";
    plan_id: string;
    entries: { cid: string; action: { type: string } }[];
  }
  interface BudgetRec {
    t: "budget";
    model: string;
    calls: number;
    prompt_tokens: number;
    completion_tokens: number;
    cost_usd: number;
  }

  type Rec = Record<string, unknown> & { t: string };

  interface LedgerRow {
    v: number;
    seq: number;
    ts: string;
    kind: string;
    crc32: number;
    prev_crc: number | null;
    rec: Rec;
  }

  let recs = $state<LedgerRow[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let restoring = $state<string | null>(null); // op_id being restored (op-scoped)
  let restored = $state<Set<number>>(new Set()); // seqs restored in this session
  let restoreErr = $state<string | null>(null);
  // D-29: countdown ticks from ledger-recorded grace_expires_at only.
  let now = $state(Date.now());

  // R5: saved agent sessions — read-only trajectory history view (no replay).
  interface SessionMeta {
    run_id: string;
    saved_at: string;
    turns: number;
    cost_usd: number;
  }
  interface TrajMsg {
    role: "system" | "user" | "assistant" | "tool";
    content: string;
    tool_calls?: { id: string; name: string; arguments: unknown }[];
  }
  interface Trajectory {
    run_id: string;
    saved_at: string;
    model: string;
    budget: { turns: number; prompt_tokens: number; completion_tokens: number; cost_usd: number };
    history: TrajMsg[];
  }

  let sessions = $state<SessionMeta[]>([]);
  let sessLoading = $state(true);
  let sessErr = $state<string | null>(null);
  let traj = $state<Trajectory | null>(null);
  let trajErr = $state<string | null>(null);

  async function loadSessions() {
    try {
      const rows = (await invoke("get_agent_sessions")) as SessionMeta[] | Record<string, unknown>;
      sessions = Array.isArray(rows) ? rows : [];
    } catch (e) {
      sessErr = String(e);
    }
    sessLoading = false;
  }

  async function openTraj(runId: string) {
    trajErr = null;
    try {
      traj = (await invoke("get_agent_trajectory", { runId })) as Trajectory;
    } catch (e) {
      traj = null;
      trajErr = String(e);
    }
  }

  let timer: ReturnType<typeof setInterval> | undefined;
  onMount(async () => {
    timer = setInterval(() => (now = Date.now()), 60_000);
    await Promise.all([refresh(), loadSessions()]);
  });
  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  async function refresh() {
    try {
      const rows = (await invoke("get_ledger_tail", { limit: 200 })) as LedgerRow[];
      recs = rows;
    } catch (e) {
      err = String(e);
    }
    loading = false;
  }

  const groups = $derived.by(() => {
    const order = ["apply", "purge", "restore", "plan", "budget", "scan", "verdict", "session"];
    const map = new Map<string, LedgerRow[]>();
    for (const r of recs) {
      const list = map.get(r.kind) ?? [];
      list.push(r);
      map.set(r.kind, list);
    }
    return order.filter((k) => map.has(k)).map((k) => ({ kind: k, rows: map.get(k)! }));
  });

  const total = $derived(recs.length);

  function isApply(r: LedgerRow): r is LedgerRow & { rec: ApplyRec } {
    return r.kind === "apply";
  }

  function opIdOf(qpath: string): string {
    // WAL op id = quarantine_path parent dir name (execute.rs restore derivation).
    const segs = qpath.split("/").filter(Boolean);
    return segs.length >= 2 ? segs[segs.length - 2] : qpath;
  }

  function graceRemaining(graceExpiresAt: string): { h: number; m: number } {
    const ms = new Date(graceExpiresAt).getTime() - now;
    if (ms <= 0) return { h: 0, m: 0 };
    return { h: Math.floor(ms / 3_600_000), m: Math.floor((ms % 3_600_000) / 60_000) };
  }

  // C3 row lifecycle. "restored" is decided by the DURABLE ledger signal — a
  // restore rec whose `from` matches this apply rec's quarantine_path (survives
  // reloads) — plus the in-session op-wide mark. "destroyed" falls out of a
  // purge rec on the same path (keeps the restore button off already-purged
  // rows, so a no-op click cannot happen for them).
  type RowState = "live" | "expired" | "restored" | "destroyed";

  const restoreRecByQpath = $derived.by(() => {
    const m = new Map<string, RestoreRec>();
    for (const r of recs) {
      if (r.kind === "restore") {
        const rr = r.rec as unknown as RestoreRec;
        if (rr.from) m.set(rr.from, rr);
      }
    }
    return m;
  });
  const purgeRecByQpath = $derived.by(() => {
    const m = new Map<string, PurgeRec>();
    for (const r of recs) {
      if (r.kind === "purge") {
        const pr = r.rec as unknown as PurgeRec;
        if (pr.quarantine_path) m.set(pr.quarantine_path, pr);
      }
    }
    return m;
  });

  function rowState(row: LedgerRow): RowState {
    const rec = row.rec as ApplyRec;
    if (restoreRecByQpath.has(rec.quarantine_path) || restored.has(row.seq)) {
      return "restored";
    }
    if (purgeRecByQpath.has(rec.quarantine_path)) {
      return "destroyed";
    }
    const rem = graceRemaining(rec.undo.grace_expires_at);
    return rem.h + rem.m > 0 ? "live" : "expired";
  }

  async function restore(row: LedgerRow) {
    const rec = row.rec as ApplyRec;
    const opId = opIdOf(rec.quarantine_path);
    restoring = opId;
    restoreErr = null;
    const ch = channel<ExecMsg>();
    ch.onmessage = () => {
      /* restore progress streams through the channel; the tail refresh below
         shows the resulting ledger `restore` record (P7: ledger is the proof). */
    };
    try {
      // restore_op resolves with { restored: string[], missing: string[],
      // collided: string[] } (demo mode: { ok, restored: number }).
      const res = (await invoke("restore_op", { opId, channel: ch })) as {
        restored: unknown;
      };
      const restoredCount =
        typeof res.restored === "number" ? res.restored : Array.isArray(res.restored) ? res.restored.length : 0;
      // Op-scoped: one click restores EVERY apply row of the op. A no-op click
      // (nothing left in quarantine — already purged/missing) marks NONE.
      if (restoredCount > 0) {
        const next = new Set(restored);
        for (const r of recs) {
          if (r.kind === "apply" && isApply(r)) {
            const rRec = r.rec as ApplyRec;
            if (opIdOf(rRec.quarantine_path) === opId) next.add(r.seq);
          }
        }
        restored = next;
      }
      await refresh();
    } catch (e) {
      restoreErr = String(e);
    }
    restoring = null;
  }

  function fmtBytes(n: number): string {
    if (!Number.isFinite(n) || n < 0) return "—";
    if (n === 0) return "0 GB";
    if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
    return `${(n / 1e3).toFixed(1)} KB`;
  }

  function fmtTs(ts: string): string {
    const d = new Date(ts);
    const today = new Date();
    const sameDay = d.toDateString() === today.toDateString();
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    return sameDay ? time : `${d.getMonth() + 1}/${d.getDate()} ${time}`;
  }

  function baseName(p: string): string {
    const segs = p.split("/").filter(Boolean);
    return segs[segs.length - 1] ?? p;
  }
</script>
{#key locale}


<div class="screen-head">
  <h1>{t("ledger.title")}</h1>
  <span class="sub num">{t("ledger.records").replace("{n}", String(total))}</span>
</div>

<div class="ledger">
  {#if err}
    <div class="refusal">
      <span class="rdot"></span>
      <div class="rt-body">
        <span class="mono">{err}</span>
      </div>
    </div>
  {:else if !loading && recs.length === 0}
    <EmptyState title={t("ledger.empty")} hint={t("ledger.empty_hint")} />
  {:else}
    {#if restoreErr}
      <div class="refusal">
        <span class="rdot"></span>
        <div>
          <div class="rt-title">{t("ledger.err_restore")}</div>
          <div class="rt-body">
            <span class="mono">{restoreErr}</span>
          </div>
        </div>
      </div>
    {/if}

    {#each groups as g (g.kind)}
      <div class="group">
        <div class="group-head">
          {t(`ledger.kind_${g.kind}`)}
          <span class="n num">{g.rows.length}</span>
        </div>
        <div class="ledger-list">
          {#each g.rows as row (row.seq)}
            {@const rec = row.rec}
            {#if row.kind === "apply" && isApply(row)}
              {@const state = rowState(row)}
              {@const rem = graceRemaining(rec.undo.grace_expires_at)}
              <div class="ledger-row">
                <span class="l-id num">#{row.seq}</span>
                <span class="l-kind k-apply">{t("ledger.kind_apply")}</span>
                <div class="l-body">
                  <div class="l-p1">
                    <span class="num">{fmtBytes(rec.undo.byte_count)}</span> · {rec.original_path}
                  </div>
                  <div class="l-p2 mono">plan-{rec.plan_id} · {fmtTs(row.ts)}</div>
                </div>
                {#if state === "live"}
                  <span class="grace num">
                    {t("ledger.remaining")} {rem.h}h {rem.m}m
                  </span>
                  <PressButton
                    kind="ghost"
                    label={t("ledger.restore")}
                    disabled={restoring !== null && restoring === opIdOf(rec.quarantine_path)}
                    onpress={() => restore(row)}
                  />
                {:else if state === "expired"}
                  <span class="l-state warn num">
                    <span class="adot" aria-hidden="true"></span>{t("ledger.pending")}
                  </span>
                {:else if state === "destroyed"}
                  <span class="l-state dead num">{t("ledger.not_restorable")}</span>
                {:else}
                  <span class="l-state ok num">{t("ledger.restored")}</span>
                {/if}
              </div>
            {:else if row.kind === "purge"}
              {@const pre = rec as unknown as PurgeRec}
              <div class="ledger-row">
                <span class="l-id num">#{row.seq}</span>
                <span class="l-kind k-purge">{t("ledger.kind_purge")}</span>
                <div class="l-body">
                  <div class="l-p1">
                    <span class="num">{fmtBytes(pre.bytes_freed)}</span> · {t("ledger.destroyed")}
                  </div>
                  <div class="l-p2 mono">
                    {t("ledger.confirmed")} {pre.bytes_freed.toLocaleString("en-US")}
                  </div>
                </div>
                <span class="l-state dead num">{t("ledger.not_restorable")}</span>
              </div>
            {:else if row.kind === "restore"}
              {@const pre = rec as unknown as RestoreRec}
              <div class="ledger-row">
                <span class="l-id num">#{row.seq}</span>
                <span class="l-kind k-restore">{t("ledger.kind_restore")}</span>
                <div class="l-body">
                  <div class="l-p1">
                    {baseName(pre.to)} → {pre.to}
                  </div>
                  <div class="l-p2 mono">
                    {t("ledger.undoes")} #{pre.undoes} · {fmtTs(row.ts)}
                  </div>
                </div>
                <span class="l-state ok num">{t("ledger.restored")}</span>
              </div>
            {:else if row.kind === "plan"}
              {@const pre = rec as unknown as PlanRec}
              <div class="ledger-row">
                <span class="l-id num">#{row.seq}</span>
                <span class="l-kind k-plan">{t("ledger.kind_plan")}</span>
                <div class="l-body">
                  <div class="l-p1 mono">plan-{pre.plan_id}</div>
                  <div class="l-p2 mono">{fmtTs(row.ts)}</div>
                </div>
              </div>
            {:else if row.kind === "budget"}
              {@const pre = rec as unknown as BudgetRec}
              <div class="ledger-row">
                <span class="l-id num">#{row.seq}</span>
                <span class="l-kind k-budget">{t("ledger.kind_budget")}</span>
                <div class="l-body">
                  <div class="l-p1">
                    {pre.model} · <span class="num">{pre.calls}</span> {t("ledger.calls")}
                  </div>
                  <div class="l-p2 mono">
                    {pre.prompt_tokens + pre.completion_tokens} tok · {fmtTs(row.ts)}
                  </div>
                </div>
                <span class="cost num">${pre.cost_usd.toFixed(4)}</span>
              </div>
            {:else}
              <div class="ledger-row dim">
                <span class="l-id num">#{row.seq}</span>
                <span class="l-kind">{row.kind}</span>
                <div class="l-body">
                  <div class="l-p2 mono">{fmtTs(row.ts)}</div>
                </div>
              </div>
            {/if}
          {/each}
        </div>
      </div>
    {/each}
  {/if}
</div>

<!-- R5: 轨迹 Trajectory — saved agent sessions, read-only history view -->
<div class="traj">
  <div class="group-head">
    {t("ledger.traj_title")}
    {#if traj}
      <button class="traj-x" onclick={() => (traj = null)}>{t("ledger.traj_close")}</button>
    {/if}
  </div>
  {#if sessErr}
    <div class="refusal">
      <span class="rdot"></span>
      <div class="rt-body">
        <span class="mono">{sessErr}</span>
      </div>
    </div>
  {:else if !sessLoading && sessions.length === 0}
    <EmptyState title={t("ledger.traj_empty")} hint={t("ledger.traj_empty_hint")} />
  {:else}
    <div class="sess-list">
      {#each sessions as s (s.run_id)}
        <button
          class="sess-row"
          class:active={traj?.run_id === s.run_id}
          onclick={() => openTraj(s.run_id)}
        >
          <span class="sess-id mono num">{s.run_id.slice(0, 8)}</span>
          <span class="sess-meta num">{fmtTs(s.saved_at)}</span>
          <span class="num">{s.turns} {t("ledger.calls")}</span>
          <span class="cost num">${s.cost_usd.toFixed(4)}</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if trajErr}
    <div class="refusal">
      <span class="rdot"></span>
      <div class="rt-body">
        <span class="mono">{trajErr}</span>
      </div>
    </div>
  {/if}
  {#if traj}
    <div class="traj-body">
      <div class="traj-meta mono">
        {traj.run_id} · {traj.model} · {fmtTs(traj.saved_at)}
      </div>
      {#each traj.history as m, i (i)}
        <div class="tmsg">
          <span class="tbadge {m.role}">{t(`ledger.traj_${m.role}`)}</span>
          <div class="tcontent">
            {#if m.tool_calls?.length}
              <div class="tcalls mono">
                {#each m.tool_calls as tc}
                  <div>→ {tc.name}({JSON.stringify(tc.arguments)})</div>
                {/each}
              </div>
            {/if}
            {#if m.content}
              <div>{m.content}</div>
            {/if}
          </div>
        </div>
      {/each}
      <div class="traj-budget num">
        {t("ledger.traj_budget")} · {traj.budget.turns} {t("ledger.calls")} ·{" "}
        {traj.budget.prompt_tokens + traj.budget.completion_tokens} {t("ledger.traj_tokens")} ·
        ${traj.budget.cost_usd.toFixed(4)}
      </div>
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

  .ledger {
    padding: 0 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    flex: 1;
    min-height: 0;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .group-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-size: 13px;
    font-weight: 600;
    padding: 2px 2px 0;
  }
  .group-head .n {
    font-size: 11px;
    color: var(--mist);
    font-weight: 400;
  }

  .ledger-list {
    border: 1px solid var(--hairline);
    border-radius: var(--r-sm);
    background: var(--surface);
    overflow: hidden;
  }
  .ledger-row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--hairline);
    font-size: 13px;
  }
  .ledger-row:last-child {
    border-bottom: none;
  }
  .ledger-row.dim {
    opacity: 0.55;
  }
  .l-id {
    color: var(--mist);
    font-size: 11px;
    flex-shrink: 0;
    width: 48px;
  }
  .l-kind {
    font-size: 10px;
    color: var(--mist);
    flex-shrink: 0;
    width: 52px;
  }
  .k-apply {
    color: var(--grade-rebuild);
  }
  .k-purge {
    color: var(--grade-hands);
  }
  .k-restore {
    color: var(--grade-pure);
  }
  .k-plan {
    color: var(--grade-redl);
  }
  .k-budget {
    color: var(--grade-expensive);
  }
  .l-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .l-p1 {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .l-p2 {
    font-size: 10px;
    color: var(--mist);
  }
  .grace {
    font-size: 11px;
    color: var(--grade-expensive);
    flex-shrink: 0;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .l-state {
    font-size: 11px;
    flex-shrink: 0;
    white-space: nowrap;
  }
  .l-state.ok {
    color: var(--grade-pure);
  }
  .l-state.warn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--grade-expensive);
  }
  .l-state.warn .adot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--grade-expensive);
    opacity: 0.55;
    flex-shrink: 0;
  }
  .l-state.dead {
    color: var(--mist);
  }
  .cost {
    font-size: 11px;
    color: var(--grade-expensive);
    flex-shrink: 0;
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

  /* R5: 轨迹 Trajectory — saved agent sessions, read-only history view */
  .traj {
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-top: 1px solid var(--hairline);
    padding-top: 16px;
  }
  .traj-x {
    margin-left: auto;
    font-size: 11px;
    color: var(--mist);
    background: none;
    border: none;
    cursor: pointer;
    padding: 2px 6px;
  }
  .traj-x:hover {
    color: var(--text);
  }
  .sess-list {
    border: 1px solid var(--hairline);
    border-radius: var(--r-sm);
    background: var(--surface);
    overflow: hidden;
  }
  .sess-row {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    padding: 8px 16px;
    border: none;
    border-bottom: 1px solid var(--hairline);
    background: none;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }
  .sess-row:last-child {
    border-bottom: none;
  }
  .sess-row:hover,
  .sess-row.active {
    background: var(--surface-lift);
  }
  .sess-id {
    color: var(--mist);
    font-size: 11px;
    flex-shrink: 0;
  }
  .sess-meta {
    color: var(--mist);
    font-size: 11px;
    flex: 1;
  }
  .traj-body {
    border: 1px solid var(--hairline);
    border-radius: var(--r-sm);
    background: var(--surface);
    overflow: hidden;
  }
  .traj-meta {
    font-size: 10px;
    color: var(--mist);
    padding: 8px 16px;
    border-bottom: 1px solid var(--hairline);
  }
  .tmsg {
    display: flex;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--hairline);
    font-size: 12px;
  }
  .tmsg:last-child {
    border-bottom: none;
  }
  .tbadge {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--mist);
    width: 52px;
  }
  .tbadge.user {
    color: var(--grade-pure);
  }
  .tbadge.assistant {
    color: var(--grade-rebuild);
  }
  .tbadge.tool {
    color: var(--grade-expensive);
  }
  .tcontent {
    flex: 1;
    min-width: 0;
    color: var(--mist);
    word-break: break-word;
    white-space: pre-wrap;
  }
  .tcalls {
    font-size: 11px;
    color: var(--text-dim);
    margin-bottom: 2px;
  }
  .traj-budget {
    padding: 8px 16px;
    font-size: 11px;
    color: var(--grade-expensive);
    border-top: 1px solid var(--hairline);
    font-variant-numeric: tabular-nums;
  }
</style>
