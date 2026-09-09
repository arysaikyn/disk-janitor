<script lang="ts">
  import { t, getLocale, onLocaleChange, formatInt } from "./i18n";
  import { invoke, channel, type ExecProgressMsg } from "./ipc";

  let { planId, bytes, onclose }: {
    planId: string;
    bytes: number;
    onclose: () => void;
  } = $props();

  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  let typed = $state("");
  let checked = $state(false);
  let attempts = $state(0);
  let mismatch = $state(false);
  // B3: the Rust Err(String) refusal surfaces verbatim instead of being
  // swallowed — a deadlocked purge must be visible, not silent.
  let error = $state<string | null>(null);
  let phase = $state<"input" | "purging" | "done">("input");
  let freed = $state<number | null>(null);

  const exact = $derived(Number(typed.replace(/[^\d]/g, "")));
  const match = $derived(exact === bytes && bytes > 0);
  const canDestroy = $derived(match && checked && phase === "input");

  function onInput(e: Event) {
    typed = (e.target as HTMLInputElement).value;
    mismatch = false;
    error = null;
  }

  // A failed attempt = submit (Enter / Destroy click) with a non-matching value.
  function noteAttempt() {
    if (typed.trim() === "" || match) return;
    attempts += 1;
    mismatch = true;
    setTimeout(() => (mismatch = false), 350);
  }

  async function destroy() {
    if (!canDestroy) {
      noteAttempt();
      return;
    }
    phase = "purging";
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
    } catch (e) {
      phase = "input";
      mismatch = true;
      // Verbatim refusal from Rust (Err(String)); Tauri may wrap it in an Error.
      error =
        typeof e === "string"
          ? e
          : e instanceof Error
            ? e.message
            : "purge failed — see application logs";
    }
  }
</script>

<!-- Deliberately NO Esc / outside-click handlers: the gate is inert by design
     (SPEC Part C §4.2 — the only exits are Cancel and the post-success Done). -->
{#key locale}
  <div class="overlay" lang={locale} role="presentation">
    <div class="modal" role="alertdialog" aria-modal="true" aria-label={t("purge.title")}>
    {#if phase === "input"}
      <h2>{t("purge.title")}</h2>
      <p class="sentence">{t("purge.irreversible")}</p>

      <div class="hero-block">
        <div class="hero num">{formatInt(bytes)}</div>
        <div class="hero-label">{t("purge.destroy")}</div>
      </div>
      <p class="restorable">{t("purge.restorable")}</p>

      <label class="understand">
        <input type="checkbox" bind:checked={checked} />
        <span>{t("purge.understand")}</span>
      </label>

      <div class="type-row" class:shake={mismatch}>
        <label class="type-label" for="purge-input">
          {t("purge.exact")} <span class="esc">— {t("purge.esc")}</span>
        </label>
        <input
          id="purge-input"
          class="num"
          type="text"
          inputmode="numeric"
          autocomplete="off"
          spellcheck="false"
          placeholder={t("purge.placeholder")}
          disabled={!checked}
          value={typed}
          oninput={onInput}
          onkeydown={(e) => e.key === "Enter" && (canDestroy ? destroy() : noteAttempt())}
        />
        {#if mismatch}
          <div class="mismatch">{t("purge.mismatch")}</div>
        {:else if attempts >= 3}
          <div class="hint num">{formatInt(bytes)}</div>
        {/if}
        {#if error}
          <div class="err" role="alert">{error}</div>
        {/if}
      </div>

      <div class="btn-row">
        <button onclick={onclose}>{t("purge.cancel")}</button>
        <button class="destroy" disabled={!canDestroy} onclick={destroy}>
          {t("purge.destroybtn")}
        </button>
      </div>
    {:else if phase === "purging"}
      <div class="progress">
        <span class="spinner" aria-hidden="true"></span>
        <span>{t("purge.progress")}</span>
      </div>
    {:else}
      <div class="hero-block done">
        <div class="hero num">{formatInt(freed ?? bytes)}</div>
        <div class="hero-label">{t("purge.freed").replace("{n}", formatInt(freed ?? bytes))}</div>
      </div>
      <div class="btn-row">
        <button class="primary" onclick={onclose}>{t("purge.finish")}</button>
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
  }
  .modal {
    width: 460px;
    background: var(--bg-raised);
    border: 1px solid var(--hairline);
    border-radius: 16px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.45);
    padding: 26px 28px;
  }
  h2 {
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.1px;
  }
  .sentence {
    font-size: 13px;
    color: var(--text-dim);
    margin-top: 4px;
  }
  .hero-block {
    margin: 20px 0 2px;
  }
  .hero {
    font-size: 34px;
    line-height: 1.15;
    color: var(--accent);
    font-weight: 600;
    letter-spacing: -1px;
  }
  .hero-label {
    font-size: 11px;
    color: var(--text-faint);
    margin-top: 2px;
  }
  .restorable {
    font-size: 11px;
    color: var(--text-faint);
    margin-bottom: 14px;
  }
  .understand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
    margin: 14px 0 16px;
    user-select: none;
  }
  .understand input {
    accent-color: var(--grade-hands);
  }
  .type-row {
    margin-bottom: 18px;
  }
  .type-label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 6px;
  }
  .esc {
    font-size: 11px;
    color: var(--text-faint);
  }
  .type-row input {
    width: 100%;
    background: var(--bg);
    border: 1px solid var(--hairline);
    border-radius: 8px;
    padding: 9px 12px;
    color: var(--text);
    font-size: 14px;
  }
  .type-row input:focus {
    border-color: var(--accent);
  }
  .mismatch {
    margin-top: 6px;
    font-size: 12px;
    color: var(--grade-hands);
  }
  .hint {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-faint);
    opacity: 0.55;
  }
  .err {
    margin-top: 6px;
    font-size: 12px;
    color: var(--grade-hands);
    word-break: break-word;
  }
  .shake {
    animation: shake 0.3s ease;
  }
  @keyframes shake {
    0%,
    100% {
      transform: translateX(0);
    }
    25% {
      transform: translateX(-6px);
    }
    75% {
      transform: translateX(6px);
    }
  }
  .btn-row {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .destroy {
    background: var(--grade-hands);
    border-color: var(--grade-hands);
    color: #ffffff;
    font-weight: 600;
  }
  .destroy:disabled {
    background: var(--bg-raised);
    border-color: var(--hairline);
    color: var(--text-dim);
    font-weight: 400;
    opacity: 0.4;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 26px 0;
    font-size: 13px;
    color: var(--text-dim);
  }
  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid var(--hairline);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .done .hero-label {
    font-size: 12px;
    color: var(--text-dim);
  }
</style>
