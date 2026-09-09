<script lang="ts">
  import { t, getLocale, onLocaleChange } from "./i18n";

  let { active, onnavigate, onsettings }: {
    active: string;
    onnavigate: (r: string) => void;
    onsettings: () => void;
  } = $props();

  // Mirror the i18n locale into local $state so templates re-render on switch.
  let locale = $state(getLocale());
  $effect(() => onLocaleChange(() => (locale = getLocale())));

  const items = [
    { id: "dashboard", key: "nav.dash", ic: "◈" },
    { id: "review", key: "nav.review", ic: "◉" },
    { id: "ledger", key: "nav.ledger", ic: "≡" },
  ];
</script>

{#key locale}
  <nav class="sidebar" lang={locale}>
    <button class="brand" onclick={() => onnavigate("dashboard")} aria-label={t("nav.dash")}>
      <span class="brand-ic">↩</span>
      <span class="brand-name">{t("brand")}</span>
    </button>

    <div class="items">
      {#each items as it (it.id)}
        <button
          class="nav-item"
          class:active={active === it.id}
          onclick={() => onnavigate(it.id)}
        >
          <span class="ic">{it.ic}</span>
          <span>{t(it.key)}</span>
        </button>
      {/each}
      <button class="nav-item" class:active={active === "settings"} onclick={onsettings}>
        <span class="ic">⚙</span>
        <span>{t("nav.settings")}</span>
      </button>
    </div>

    <div class="foot">
      <div>{t("sidebar.foot")}</div>
      <div class="foot2">{t("sidebar.foot2")}</div>
    </div>
  </nav>
{/key}

<style>
  .sidebar {
    width: 200px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--hairline);
    background: var(--bg);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 36px 18px 14px; /* top clears the transparent titlebar traffic lights */
    border: none;
    background: transparent;
    color: var(--text);
    font-size: 14px;
    font-weight: 600;
    text-align: left;
    letter-spacing: -0.1px;
  }
  .brand-ic {
    color: var(--accent);
    font-size: 17px;
    line-height: 1;
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 8px;
  }
  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: none;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    text-align: left;
  }
  .nav-item:hover {
    color: var(--text);
  }
  .nav-item.active {
    color: var(--accent);
    font-weight: 500;
  }
  .nav-item.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 6px;
    bottom: 6px;
    width: 2px;
    border-radius: 1px;
    background: var(--accent);
  }
  .nav-item .ic {
    width: 16px;
    text-align: center;
    opacity: 0.85;
  }
  .foot {
    margin-top: auto;
    padding: 12px 18px 16px;
    font-size: 10px;
    color: var(--text-faint);
    line-height: 1.7;
  }
</style>
