<script lang="ts">
  import { onMount } from "svelte";
  import { t, getLocale, setLocale, onLocaleChange } from "./i18n";
  import { invoke } from "./ipc";
  import PressButton from "./PressButton.svelte";

  let {
    onclose,
    onMinBytes,
  }: {
    onclose: () => void;
    /** Live 最小候选大小 hook — fires on change with the byte value (50/100/500). */
    onMinBytes?: (bytes: number) => void;
  } = $props();

  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  // Local-only state (thresholds live in config.toml, SPEC Part C §3.8).
  let minBytes = $state<"50" | "100" | "500">("100");
  let theme = $state<"system" | "light" | "dark">("system");

  // ── 模型 section (R3) — lives in config.toml, SPEC Part C §3.9 ────────────
  // Loaded from get_model_config on sheet open; saved through save_model_config.
  // The API key never leaves the host (P4): only `has_key` comes back, and a
  // blank key field on save means "keep the existing key" (Rust: None).
  interface ModelConfigDto {
    endpoint: string;
    has_key: boolean;
    model: string;
    ctx_len: number;
    thinking: boolean;
    budget: { cost_usd: number; max_turns: number; tokens: number };
  }

  let model = $state<ModelConfigDto | null>(null); // null = still loading
  let endpoint = $state("");
  let apiKey = $state("");
  let modelName = $state("");
  let ctxLen = $state("");
  let budgetUsd = $state("");
  let thinking = $state(false);
  let saving = $state(false);
  let savedFlash = $state(false);
  let modelError = $state("");

  onMount(() => {
    invoke<ModelConfigDto>("get_model_config", {})
      .then((c) => {
        model = c;
        endpoint = c.endpoint ?? "";
        modelName = c.model ?? "";
        ctxLen = c.ctx_len != null ? String(c.ctx_len) : "";
        budgetUsd = c.budget?.cost_usd != null ? String(c.budget.cost_usd) : "";
        thinking = !!c.thinking;
      })
      .catch((e) => {
        modelError = String(e);
      });
  });

  async function saveModel() {
    const cur = model;
    if (!cur || saving) return;
    modelError = "";
    savedFlash = false;

    const ctx = Number(ctxLen);
    if (ctxLen.trim() === "" || !Number.isInteger(ctx) || ctx < 0) {
      modelError = t("settings.model_ctx_invalid");
      return;
    }
    const usd = Number(budgetUsd);
    if (budgetUsd.trim() === "" || !Number.isFinite(usd) || usd < 0) {
      modelError = t("settings.model_budget_invalid");
      return;
    }

    const key = apiKey.trim(); // blank ⇒ keep the existing key (Rust: None)
    saving = true;
    try {
      await invoke("save_model_config", {
        endpoint,
        apiKey: key === "" ? null : key,
        model: modelName,
        ctxLen: ctx,
        thinking,
        budget: {
          cost_usd: usd,
          // Demo/partial DTOs may lack budget rows — fall back to Config defaults.
          max_turns: cur.budget?.max_turns ?? 12,
          tokens: cur.budget?.tokens ?? 200_000,
        },
      });
      if (key !== "") cur.has_key = true;
      apiKey = "";
      savedFlash = true;
      setTimeout(() => (savedFlash = false), 2400);
    } catch (e) {
      modelError = String(e);
    } finally {
      saving = false;
    }
  }

  // Exit runs 20% faster than enter (220ms × 0.8 ≈ 180ms): flip the transform,
  // let the transition finish, then let onclose unmount the sheet.
  let closing = $state(false);
  function requestClose() {
    if (closing) return;
    closing = true;
    setTimeout(onclose, 180);
  }

  const MIN_OPTS: Array<"50" | "100" | "500"> = ["50", "100", "500"];
  const THEMES: Array<{ key: "system" | "light" | "dark"; labelKey: string }> = [
    { key: "system", labelKey: "settings.followsys" },
    { key: "light", labelKey: "settings.light" },
    { key: "dark", labelKey: "settings.dark" },
  ];

  function setMin(v: "50" | "100" | "500") {
    minBytes = v;
    onMinBytes?.(Number(v));
  }

  $effect(() => {
    const el = document.documentElement;
    if (theme === "system") delete el.dataset.theme;
    else el.dataset.theme = theme;
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && requestClose()} />

{#key locale}
  <div
    class="overlay"
    class:closing={closing}
    lang={locale}
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) requestClose();
    }}
  >
    <div class="sheet" role="dialog" aria-modal="true" aria-label={t("settings.title")}>
      <header class="sheet-head hairline-b">
        <h2>{t("settings.title")}</h2>
        <button class="close" aria-label={t("common.close")} onclick={requestClose}>✕</button>
      </header>

      <div class="body">
        <section class="group">
          <h3 class="g-label">{t("settings.scan")}</h3>

          <div class="row">
            <span class="row-label" id="min-bytes-label">{t("settings.mincandidate")}</span>
            <div class="seg" role="group" aria-labelledby="min-bytes-label">
              {#each MIN_OPTS as v (v)}
                <span class="seg-tab" class:active={minBytes === v}>
                  <PressButton kind="ghost" label={`${v} ${t("unit.mb")}`} onpress={() => setMin(v)} />
                </span>
              {/each}
            </div>
          </div>

          <div class="row">
            <span class="row-label">{t("settings.roots")}</span>
            <span class="row-value">~/</span>
          </div>

          <div class="row">
            <span class="row-label">{t("settings.ignores")}</span>
            <span class="row-value">node_modules, *.log</span>
          </div>

          <div class="sub">{t("settings.m4_hint")}</div>
        </section>

        <section class="group">
          <h3 class="g-label">{t("settings.model")}</h3>

          <div class="row">
            <span class="row-label">{t("settings.model_endpoint")}</span>
            <input
              class="field mono"
              type="text"
              spellcheck="false"
              autocomplete="off"
              placeholder="https://api.deepseek.com/v1"
              bind:value={endpoint}
            />
          </div>

          <div class="row">
            <span class="row-label">{t("settings.model_apikey")}</span>
            <input
              class="field"
              type="password"
              autocomplete="off"
              spellcheck="false"
              placeholder={model?.has_key ? t("settings.model_apikey_set") : t("settings.model_apikey_unset")}
              bind:value={apiKey}
            />
          </div>

          <div class="row">
            <span class="row-label">{t("settings.model_name")}</span>
            <input
              class="field mono"
              type="text"
              spellcheck="false"
              autocomplete="off"
              bind:value={modelName}
            />
          </div>

          <div class="row">
            <span class="row-label">{t("settings.model_ctx")}</span>
            <input
              class="field num"
              type="text"
              inputmode="numeric"
              autocomplete="off"
              spellcheck="false"
              bind:value={ctxLen}
            />
          </div>

          <div class="row">
            <span class="row-label">{t("settings.model_thinking")}</span>
            <div class="seg" role="group" aria-label={t("settings.model_thinking")}>
              <span class="seg-tab" class:active={thinking}>
                <PressButton kind="ghost" label={t("settings.model_thinking_on")} onpress={() => (thinking = true)} />
              </span>
              <span class="seg-tab" class:active={!thinking}>
                <PressButton kind="ghost" label={t("settings.model_thinking_off")} onpress={() => (thinking = false)} />
              </span>
            </div>
          </div>

          <div class="row">
            <span class="row-label">{t("settings.model_budget")}</span>
            <input
              class="field num"
              type="text"
              inputmode="decimal"
              autocomplete="off"
              spellcheck="false"
              bind:value={budgetUsd}
            />
          </div>

          <div class="save-row">
            <PressButton
              kind="primary"
              label={t("common.save")}
              disabled={!model || saving}
              onpress={saveModel}
            />
            {#if savedFlash}
              <PressButton kind="ghost" label={t("settings.model_saved")} />
            {/if}
          </div>
          {#if modelError}
            <div class="err" role="alert">{modelError}</div>
          {/if}
        </section>

        <section class="group">
          <h3 class="g-label">{t("settings.language")}</h3>
          <div class="control-row">
            <div class="seg" role="group" aria-label={t("settings.language")}>
              <span class="seg-tab" class:active={locale === "zh"}>
                <PressButton kind="ghost" label={t("settings.zh")} onpress={() => setLocale("zh")} />
              </span>
              <span class="seg-tab" class:active={locale === "en"}>
                <PressButton kind="ghost" label={t("settings.en")} onpress={() => setLocale("en")} />
              </span>
            </div>
          </div>
        </section>

        <section class="group">
          <h3 class="g-label">{t("settings.appearance")}</h3>
          <div class="control-row">
            <div class="seg" role="group" aria-label={t("settings.appearance")}>
              {#each THEMES as opt (opt.key)}
                <span class="seg-tab" class:active={theme === opt.key}>
                  <PressButton kind="ghost" label={t(opt.labelKey)} onpress={() => (theme = opt.key)} />
                </span>
              {/each}
            </div>
          </div>
        </section>

        <section class="group">
          <h3 class="g-label">{t("settings.privacy")}</h3>
          <p class="privacy-note">{t("settings.privacy_note")}</p>
          <p class="privacy-sub">{t("settings.privacy_sub")}</p>
        </section>

        <section class="group">
          <h3 class="g-label">{t("settings.uninstall")}</h3>
          <div class="uninstall">
            <PressButton kind="danger" label={t("settings.uninstall_btn")} disabled />
            <span class="sub">{t("settings.uninstall_note")}</span>
          </div>
        </section>
      </div>
    </div>
  </div>
{/key}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(4, 8, 13, 0.55);
    z-index: 60;
    display: flex;
    justify-content: flex-end;
    transition: opacity var(--t-med) var(--ease);
  }
  .overlay.closing {
    opacity: 0;
  }
  .sheet {
    width: 420px;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--r-lg) 0 0 var(--r-lg);
    box-shadow: var(--shadow-3);
    animation: sheet-in var(--t-med) var(--ease);
  }
  @keyframes sheet-in {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
  .overlay.closing .sheet {
    transform: translateX(24px);
    opacity: 0;
    transition:
      transform 180ms var(--ease),
      opacity 180ms var(--ease);
  }

  .sheet-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
  }
  .sheet-head h2 {
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.1px;
  }
  .close {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    border-radius: var(--r-sm);
    color: var(--mist);
    font-size: 13px;
  }
  .close:hover {
    background: var(--surface-lift);
    color: var(--text);
  }

  .body {
    flex: 1;
    overflow-y: auto;
  }

  .group {
    padding: 18px 20px;
    border-bottom: 1px solid var(--hairline);
  }
  .group:last-child {
    border-bottom: none;
  }
  .g-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--mist);
    margin-bottom: 12px;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 44px;
  }
  .row-label {
    font-size: 13px;
    color: var(--text);
  }
  .row-value {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--mist);
    font-variant-numeric: tabular-nums;
  }
  .control-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    min-height: 44px;
  }
  .sub {
    font-size: 12px;
    color: var(--mist);
    line-height: 1.5;
    margin-top: 6px;
  }

  /* segmented control — container owns the hairline ring; tabs are plain
     PressButton-ghosts; active = teal text + 2px teal underline, no fill */
  .seg {
    display: inline-flex;
    align-items: stretch;
    gap: 1px;
    padding: 2px;
    border-radius: var(--r-sm);
    box-shadow: inset 0 0 0 1px var(--hairline);
  }
  .seg :global(button.press) {
    min-height: 36px;
    padding: 0 12px;
    border-radius: calc(var(--r-sm) - 2px);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .seg :global(button.press.ghost) {
    box-shadow: none;
  }
  .seg-tab.active :global(button.press) {
    color: var(--accent);
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .seg-tab.active :global(button.press):hover {
    background: transparent;
  }

  .privacy-note {
    font-size: 13px;
    color: var(--text);
    line-height: 1.7;
  }
  .privacy-sub {
    font-size: 12px;
    color: var(--mist);
    margin-top: 2px;
    line-height: 1.6;
  }

  .uninstall {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  /* 模型 section — text/number inputs ride the global input style (surface bg,
     hairline ring, focus-visible accent); these only size/align them. */
  .field {
    width: 220px;
    min-width: 0;
    font-size: 13px;
  }
  .field.mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  .field.num {
    text-align: right;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
  }
  .save-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
  }
  .err {
    margin-top: 10px;
    font-size: 12px;
    color: var(--danger);
    line-height: 1.5;
    word-break: break-word;
  }

  /* explicit light/dark override (app.css only handles system mode) */
  :global(html[data-theme="light"]) {
    --bg: #f5f7fa;
    --bg-raised: #ffffff;
    --hairline: #dfe5ec;
    --text: #1a2430;
    --text-dim: #5c6875;
    --text-faint: #97a3b1;
  }
  :global(html[data-theme="dark"]) {
    --bg: #0c1118;
    --bg-raised: #111823;
    --hairline: #1c2634;
    --text: #dbe4ee;
    --text-dim: #8b98a8;
    --text-faint: #5c6875;
  }
</style>
