<script lang="ts">
  import { t, getLocale, setLocale, onLocaleChange } from "./i18n";

  let { onclose }: { onclose: () => void } = $props();

  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  // Local-only state (thresholds live in config.toml, SPEC Part C §3.8).
  let minBytes = $state<"50" | "100" | "500">("50");
  let theme = $state<"system" | "light" | "dark">("system");

  $effect(() => {
    const el = document.documentElement;
    if (theme === "system") delete el.dataset.theme;
    else el.dataset.theme = theme;
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

{#key locale}
  <div
    class="overlay"
    lang={locale}
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <div class="sheet" role="dialog" aria-modal="true" aria-label={t("settings.title")}>
    <header class="sheet-head hairline-b">
      <h2>{t("settings.title")}</h2>
      <button class="close" aria-label={t("common.close")} onclick={onclose}>✕</button>
    </header>

    <div class="body">
      <section class="group">
        <div class="g-label">{t("settings.scan")}</div>
        <div class="field">
          <label for="min-bytes">{t("settings.mincandidate")}</label>
          <select id="min-bytes" bind:value={minBytes}>
            <option value="50">{t("unit.mb")} 50</option>
            <option value="100">100 {t("unit.mb")}</option>
            <option value="500">500 {t("unit.mb")}</option>
          </select>
        </div>
        <div class="field">
          <label for="settings-roots">{t("settings.roots")}</label>
          <input id="settings-roots" type="text" value="~/" disabled />
          <span class="m4">{t("settings.m4")}</span>
        </div>
        <div class="field">
          <label for="settings-ignores">{t("settings.ignores")}</label>
          <input id="settings-ignores" type="text" value="node_modules, *.log" disabled />
          <span class="m4">{t("settings.m4")}</span>
        </div>
      </section>

      <section class="group">
        <div class="g-label">{t("settings.language")}</div>
        <div class="radio-row">
          <label>
            <input type="radio" name="lang" checked={locale === "zh"} onchange={() => setLocale("zh")} />
            {t("settings.zh")}
          </label>
          <label>
            <input type="radio" name="lang" checked={locale === "en"} onchange={() => setLocale("en")} />
            {t("settings.en")}
          </label>
        </div>
      </section>

      <section class="group">
        <div class="g-label">{t("settings.appearance")}</div>
        <div class="radio-row">
          <label>
            <input type="radio" name="theme" checked={theme === "system"} onchange={() => (theme = "system")} />
            {t("settings.followsys")}
          </label>
          <label>
            <input type="radio" name="theme" checked={theme === "light"} onchange={() => (theme = "light")} />
            {t("settings.light")}
          </label>
          <label>
            <input type="radio" name="theme" checked={theme === "dark"} onchange={() => (theme = "dark")} />
            {t("settings.dark")}
          </label>
        </div>
      </section>

      <section class="group">
        <div class="g-label">{t("settings.privacy")}</div>
        <div class="privacy-note">{t("settings.privacy_note")}</div>
        <div class="privacy-sub">{t("settings.privacy_sub")}</div>
      </section>

      <section class="group">
        <div class="g-label">{t("settings.uninstall")}</div>
        <div class="uninstall-row">
          <button disabled>{t("settings.uninstall")}</button>
          <span class="note">{t("settings.uninstall_note")}</span>
        </div>
      </section>
    </div>

    <footer class="sheet-foot hairline-b">
      <button onclick={onclose}>{t("settings.close")}</button>
    </footer>
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
  }
  .sheet {
    width: 420px;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border-left: 1px solid var(--hairline);
    box-shadow: -20px 0 60px rgba(0, 0, 0, 0.35);
    animation: sheet-in 0.18s ease;
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
  .sheet-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
  }
  .sheet-head h2 {
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.1px;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--text-faint);
    font-size: 13px;
    padding: 4px 8px;
  }
  .close:hover {
    color: var(--text);
  }
  .body {
    flex: 1;
    overflow-y: auto;
  }
  .group {
    padding: 14px 18px;
    border-bottom: 1px solid var(--hairline);
  }
  .g-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-faint);
    margin-bottom: 10px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .field label {
    flex: 0 0 96px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .field input,
  .field select {
    flex: 1;
    min-width: 0;
    background: var(--bg);
    border: 1px solid var(--hairline);
    border-radius: 7px;
    padding: 6px 10px;
    color: var(--text);
    font-size: 12px;
    font-family: var(--mono);
  }
  .field input:disabled {
    opacity: 0.45;
  }
  .m4 {
    flex: 0 0 auto;
    font-size: 10px;
    color: var(--text-faint);
    border: 1px solid var(--hairline);
    border-radius: 4px;
    padding: 1px 5px;
  }
  .radio-row {
    display: flex;
    gap: 16px;
    font-size: 13px;
    color: var(--text);
  }
  .radio-row label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  input[type="radio"] {
    accent-color: var(--accent);
  }
  .privacy-note {
    font-size: 13px;
    color: var(--text);
    line-height: 1.7;
  }
  .privacy-sub {
    font-size: 11px;
    color: var(--text-faint);
    margin-top: 2px;
  }
  .uninstall-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .note {
    font-size: 11px;
    color: var(--text-faint);
    line-height: 1.6;
  }
  .sheet-foot {
    padding: 14px 18px;
    display: flex;
    justify-content: flex-end;
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
