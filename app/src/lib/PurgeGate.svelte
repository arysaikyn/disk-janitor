<script lang="ts">
  import { t, getLocale, onLocaleChange, formatInt } from "./i18n";
  import { invoke, channel, type ExecProgressMsg } from "./ipc";
  import Orb from "./Orb.svelte";
  import PressButton from "./PressButton.svelte";

  let { planId, bytes, onclose }: {
    planId: string;
    bytes: number;
    onclose: () => void;
  } = $props();

  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  let typed = $state("");
  let checked = $state(false);
  let mismatch = $state(false);
  // B3: the Rust Err(String) refusal surfaces verbatim instead of being
  // swallowed — a deadlocked purge must be visible, not silent.
  let error = $state<string | null>(null);
  let phase = $state<"input" | "purging" | "done">("input");
  let freed = $state<number | null>(null);
  // THE ORB IS THE GATE: idle while empty/typing toward the count, brief amber
  // flicker on a wrong count, success when the typed number matches (button
  // unlocks), working while the real purge runs.
  let orbState = $state<"idle" | "working" | "success" | "error">("idle");

  // modal enter/exit, reduced-motion, feedback bookkeeping
  let shown = $state(false);
  let exiting = $state(false);
  let reduced = $state(false);
  let shaking = $state(false);
  let flashing = $state(false);
  let inputEl: HTMLInputElement | undefined = $state();
  let settleTimer: ReturnType<typeof setTimeout> | null = null;
  let timers: ReturnType<typeof setTimeout>[] = [];

  const EXIT_MS = 176; // enter is --t-med (220ms); exits run 20% faster
  const FLICKER_MS = 600; // brief error hold (orb flicker itself is 450ms), then calm

  const exact = $derived(Number(typed.replace(/[^\d]/g, "")));
  const typedDigits = $derived(typed.replace(/[^\d]/g, ""));
  const targetDigits = $derived(String(bytes));
  const match = $derived(exact === bytes && bytes > 0);
  const canDestroy = $derived(match && checked && phase === "input");
  // "typing toward" the count = current digits are a proper prefix of the exact
  // count — still typing, no judgment yet (empty also stays neutral).
  const typingToward = $derived(
    typedDigits.length > 0 &&
      typedDigits.length < targetDigits.length &&
      targetDigits.startsWith(typedDigits),
  );
  // live freed counter under the orb: hidden while typing, ticking during purge
  const orbCount = $derived(
    phase === "input" ? null : phase === "purging" ? (freed ?? 0) : (freed ?? bytes),
  );

  function later(fn: () => void, ms: number): void {
    const t = setTimeout(() => {
      timers = timers.filter((x) => x !== t);
      fn();
    }, ms);
    timers.push(t);
  }

  // timer hygiene: nothing fires after unmount
  $effect(() => {
    return () => {
      if (settleTimer) clearTimeout(settleTimer);
      for (const t of timers) clearTimeout(t);
    };
  });

  // prefers-reduced-motion: orb static states, no shake — the input's hairline
  // ring flashes danger instead (PLAN motion rules)
  $effect(() => {
    if (typeof window === "undefined") return;
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    reduced = mq.matches;
    const onChange = (e: MediaQueryListEvent) => (reduced = e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  });

  // enter: scale 0.96→1 + fade, 220ms (--t-med); reduced-motion zeroes it
  $effect(() => {
    later(() => (shown = true), 30);
  });

  /** Wrong count: brief error flicker + 150ms input shake, then back to calm. */
  function refuse(): void {
    orbState = "error";
    refuseShake();
    if (settleTimer) clearTimeout(settleTimer);
    settleTimer = setTimeout(() => {
      settleTimer = null;
      orbState = match ? "success" : "idle";
    }, FLICKER_MS);
  }

  function refuseShake(): void {
    if (reduced) {
      flashing = true;
      later(() => (flashing = false), 300);
      return;
    }
    // restart the 150ms shake per wrong keystroke: drop the class, force a
    // reflow, re-add (Svelte 5 applies class directives synchronously)
    shaking = false;
    void inputEl?.offsetWidth;
    shaking = true;
  }

  /** The orb judges every keystroke: match → success, empty/prefix → idle, else refuse. */
  function evaluate(): void {
    if (settleTimer) {
      clearTimeout(settleTimer);
      settleTimer = null;
    }
    if (match) {
      orbState = "success";
      return;
    }
    if (typedDigits === "" || typingToward) {
      orbState = "idle";
      return;
    }
    refuse();
  }

  function onInput(e: Event) {
    typed = (e.target as HTMLInputElement).value;
    mismatch = false;
    error = null;
    evaluate();
  }

  // A failed submit (Enter / Destroy click with a non-matching value) is refused:
  // the mismatch verdict stays visible briefly on top of the flicker + shake.
  function noteAttempt() {
    if (typed.trim() === "" || match) return;
    mismatch = true;
    later(() => (mismatch = false), 350);
    refuse();
  }

  async function destroy() {
    if (!canDestroy) {
      noteAttempt();
      return;
    }
    phase = "purging";
    orbState = "working";
    const ch = channel<ExecProgressMsg>((m) => {
      if (m.t === "purged") freed = (freed ?? 0) + m.bytes;
    });
    try {
      const res = await invoke<{ bytes_freed?: number; freed_bytes?: number }>("purge_plan", {
        planId,
        typedBytes: bytes,
        channel: ch,
      });
      freed = res?.bytes_freed ?? res?.freed_bytes ?? bytes;
      phase = "done";
      orbState = "success";
    } catch (e) {
      phase = "input";
      mismatch = true;
      later(() => (mismatch = false), 350);
      refuse();
      // Verbatim refusal from Rust (Err(String)); Tauri may wrap it in an Error.
      error =
        typeof e === "string"
          ? e
          : e instanceof Error
            ? e.message
            : "purge failed — see application logs";
    }
  }

  /** Cancel / Done — play the exit (20% faster) before calling onclose. */
  function close() {
    if (exiting) return;
    exiting = true;
    later(onclose, EXIT_MS);
  }
</script>

<!-- Deliberately NO Esc / outside-click handlers: the gate is inert by design
     (SPEC Part C §4.2 — the only exits are Cancel and the post-success Done). -->
{#key locale}
  <div class="overlay" lang={locale} role="presentation" class:shown>
    <div
      class="modal"
      class:shown
      class:exit={exiting}
      role="alertdialog"
      aria-modal="true"
      aria-label={t("purge.title")}
    >
      {#if phase === "input"}
        <h2>{t("purge.title")}</h2>
        <p class="sentence">{t("purge.irreversible")}</p>

        <!-- the exact count to type — always visible (P4 honesty); the orb
             below is the feedback, not the puzzle -->
        <div class="hero">
          <div class="hero-cap">{t("purge.destroy")}</div>
          <div class="hero-num num">{formatInt(bytes)}</div>
        </div>

        <div class="orb-slot">
          <Orb size={160} state={orbState} count={orbCount} ariaLabel={t("orb." + orbState)} />
        </div>

        <p class="restorable">{t("purge.restorable")}</p>

        <label class="understand">
          <input type="checkbox" bind:checked={checked} />
          <span>{t("purge.understand")}</span>
        </label>

        <div class="type-row">
          <label class="type-label" for="purge-input">
            {t("purge.exact")} <span class="esc">— {t("purge.esc")}</span>
          </label>
          <input
            id="purge-input"
            class="num"
            class:shake={shaking}
            class:flash-danger={flashing}
            type="text"
            inputmode="numeric"
            autocomplete="off"
            spellcheck="false"
            placeholder={t("purge.placeholder")}
            disabled={!checked}
            value={typed}
            bind:this={inputEl}
            oninput={onInput}
            onkeydown={(e) => e.key === "Enter" && (canDestroy ? destroy() : noteAttempt())}
          />
          {#if mismatch}
            <div class="mismatch">{t("purge.mismatch")}</div>
          {/if}
          {#if error}
            <div class="err" role="alert">{error}</div>
          {/if}
        </div>

        <div class="btn-row">
          <PressButton kind="ghost" label={t("purge.cancel")} onpress={close} />
          <PressButton
            kind="primary"
            label={t("purge.destroybtn")}
            disabled={!canDestroy}
            onpress={destroy}
          />
        </div>
      {:else if phase === "purging"}
        <h2>{t("purge.title")}</h2>
        <div class="hero">
          <div class="hero-cap">{t("purge.destroy")}</div>
          <div class="hero-num num">{formatInt(bytes)}</div>
        </div>
        <div class="orb-slot">
          <Orb
            size={160}
            state="working"
            rate={0.8}
            count={orbCount}
            ariaLabel={t("orb.working")}
          />
        </div>
        <p class="working-cap">{t("purge.progress")}</p>
      {:else}
        <h2>{t("purge.title")}</h2>
        <div class="orb-slot">
          <Orb size={160} state="success" count={orbCount} ariaLabel={t("orb.success")} />
        </div>
        <p class="freed-line">
          {t("purge.freed_pre")} <span class="num">{formatInt(freed ?? bytes)}</span>
          {t("purge.freed_post")}
        </p>
        <div class="btn-row">
          <PressButton kind="primary" label={t("purge.finish")} onpress={close} />
        </div>
      {/if}
    </div>
  </div>
{/key}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(4, 8, 13, 0.6);
    z-index: 70;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity var(--t-med) var(--ease);
  }
  .overlay.shown {
    opacity: 1;
  }
  .modal {
    width: 460px;
    background: var(--surface);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-3);
    padding: 28px 32px;
    opacity: 0;
    transform: scale(0.96);
    transition:
      opacity var(--t-med) var(--ease),
      transform var(--t-med) var(--ease);
  }
  .modal.shown {
    opacity: 1;
    transform: scale(1);
  }
  /* exit 20% faster than enter (220ms → 176ms) */
  .modal.exit {
    opacity: 0;
    transform: scale(0.96);
    transition:
      opacity 176ms var(--ease),
      transform 176ms var(--ease);
  }
  h2 {
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.1px;
  }
  .sentence {
    font-size: 13px;
    color: var(--mist);
    margin-top: 4px;
  }
  .hero {
    margin: 22px 0 16px;
    text-align: center;
  }
  .hero-cap {
    font-size: 12px;
    color: var(--mist);
    letter-spacing: 0.01em;
  }
  .hero-num {
    font-size: 34px;
    font-weight: 600;
    line-height: 1.15;
    letter-spacing: -0.01em;
    margin-top: 4px;
  }
  .orb-slot {
    display: flex;
    justify-content: center;
  }
  .restorable {
    font-size: 12px;
    color: var(--text-faint);
    text-align: center;
    margin: 12px 0 4px;
  }
  .understand {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    font-size: 13px;
    color: var(--text);
    margin: 14px 0 10px;
    user-select: none;
  }
  .understand input {
    accent-color: var(--danger);
  }
  .type-row {
    margin-bottom: 18px;
  }
  .type-label {
    display: block;
    font-size: 12px;
    color: var(--mist);
    margin-bottom: 6px;
  }
  .esc {
    font-size: 11px;
    color: var(--text-faint);
  }
  .type-row input {
    width: 100%;
    background: var(--ink);
    font-size: 14px;
  }
  .type-row input:focus {
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .type-row input:disabled {
    opacity: 0.55;
    cursor: default;
  }
  /* wrong count: 150ms horizontal shake; reduced-motion flashes the ring instead */
  .type-row input.shake {
    animation: shake-x 150ms var(--ease);
  }
  .type-row input.flash-danger {
    box-shadow: inset 0 0 0 1px var(--danger);
  }
  @keyframes shake-x {
    0%,
    100% {
      transform: translateX(0);
    }
    30% {
      transform: translateX(-5px);
    }
    65% {
      transform: translateX(4px);
    }
  }
  .mismatch {
    margin-top: 6px;
    font-size: 12px;
    color: var(--danger);
  }
  .err {
    margin-top: 6px;
    font-size: 12px;
    color: var(--danger);
    word-break: break-word;
  }
  .btn-row {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 20px;
  }
  .working-cap {
    margin-top: 14px;
    text-align: center;
    font-size: 13px;
    color: var(--mist);
  }
  .freed-line {
    margin-top: 16px;
    text-align: center;
    font-size: 16px;
    color: var(--text);
  }
  .freed-line .num {
    font-weight: 600;
  }
</style>
