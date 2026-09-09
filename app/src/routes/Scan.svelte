<script lang="ts">
  import { onMount } from "svelte";
  import { t, getLocale, onLocaleChange } from "../lib/i18n";
  import { invoke, channel } from "../lib/ipc";

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

  let pct = $state(0);
  let indeterminate = $state(true);
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

  // Rust sends Finished BEFORE start_scan resolves; the demo resolves early and
  // streams Finished later. Awaiting the event keeps the ring on screen until the
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
          // Rust ScanProgress::Files carries no total, so a determinate % is only
          // possible when the transport supplies one (demo layer); otherwise the
          // ring runs indeterminate and jumps to 100% on Finished — never a fake number.
          if (typeof msg.total === "number" && msg.total > 0) {
            indeterminate = false;
            pct = Math.min(99, (msg.bytes / msg.total) * 100);
          }
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
          indeterminate = false;
          pct = 100;
          resolveFinished?.();
          break;
      }
    };
    try {
      // Default root: home dir; tilde expansion is the host's concern (R1 demo path).
      const path = await invoke("start_scan", { roots: ["~"], minBytes: null, channel: ch });
      await Promise.race([finishedEvent, new Promise((r) => setTimeout(r, 60_000))]);
      localStorage.setItem("last-scan-path", String(path));
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
  <span class="sub mono">~</span>
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
    <div class="ring">
      <svg width="210" height="210" viewBox="0 0 210 210">
        <circle class="bg-ring" cx="105" cy="105" r="92" fill="none" stroke-width="13" />
        <circle
          class="fg-ring {indeterminate ? 'indet' : ''}"
          cx="105"
          cy="105"
          r="92"
          fill="none"
          stroke-width="13"
          stroke-dasharray={indeterminate ? "120 458" : "578.05"}
          stroke-dashoffset={578.05 * (1 - pct / 100)}
        />
      </svg>
      <div class="core">
        <div class="pct num">{Math.round(pct)}%</div>
        <div class="n num">{fmtCount(files)}</div>
        <div class="n lbl">{finished ? t("scan.done") : t("scan.scanned")}</div>
      </div>
    </div>

    <div class="scan-side">
      {#if current}
        <div class="current mono">{current}</div>
      {/if}
      <div class="rate num">
        {fmtGiB(rate)}<small>GiB{t("scan.rate_unit")}</small>
      </div>
      <div class="meta">
        <span class="num">{candidates} {t("scan.candidates")} · {fmtBytes(candBytes)}</span>
        {#if rootErrs.length > 0}
          <span class="num err">{rootErrs.length} {t("scan.root_errs")}</span>
        {/if}
      </div>
      <div class="cancel-zone">
        <button disabled={canceling || finished} onclick={cancel}>{t("scan.cancel")}</button>
        <span class="note">{t("scan.cancel_note")}</span>
      </div>
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
  .sub {
    font-size: 12px;
    color: var(--text-faint);
  }

  .scan-stage {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 56px;
    padding: 12px 40px 20px;
    min-height: 0;
  }

  .ring {
    position: relative;
    width: 210px;
    height: 210px;
    flex-shrink: 0;
  }
  .ring svg {
    transform: rotate(-90deg);
  }
  .bg-ring {
    stroke: var(--bg-raised);
  }
  .fg-ring {
    stroke: var(--accent);
    stroke-linecap: round;
    transition: stroke-dashoffset 0.25s cubic-bezier(0.32, 0.72, 0, 1);
  }
  .fg-ring.indet {
    stroke-dashoffset: 0;
    animation: ring-spin 1.6s linear infinite;
    transform-origin: center;
  }
  @keyframes ring-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .ring .core {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
  }
  .core .pct {
    font-size: 38px;
    font-weight: 700;
    letter-spacing: -1px;
  }
  .core .n {
    font-size: 12px;
    color: var(--text-dim);
  }
  .core .lbl {
    font-size: 10px;
    color: var(--text-faint);
  }

  .scan-side {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 280px;
  }
  .current {
    font-size: 11px;
    color: var(--text-faint);
    word-break: break-all;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--bg-raised);
    border: 1px solid var(--hairline);
    max-width: 420px;
    max-height: 64px;
    overflow: hidden;
  }
  .rate {
    font-size: 22px;
    font-weight: 650;
  }
  .rate small {
    font-size: 11px;
    color: var(--text-faint);
    font-weight: 400;
    margin-left: 6px;
  }
  .meta {
    display: flex;
    gap: 18px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .meta .err {
    color: var(--grade-expensive);
  }
  .cancel-zone {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 4px;
  }
  .cancel-zone .note {
    font-size: 10px;
    color: var(--text-faint);
  }

  .cancel-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 28px 10px;
    padding: 10px 14px;
    border: 1px solid var(--hairline);
    border-left: 2px solid var(--grade-expensive);
    border-radius: 8px;
    background: var(--bg-raised);
    font-size: 12px;
    color: var(--text-dim);
  }

  .root-errs {
    margin: 0 28px 18px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .root-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 11px;
    padding: 6px 12px;
    border: 1px solid var(--hairline);
    border-radius: 6px;
    background: var(--bg-raised);
  }
  .root-row .path {
    color: var(--text-dim);
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
    border-radius: 8px;
    padding: 12px 14px;
    background: var(--bg-raised);
    font-size: 12px;
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
</style>
