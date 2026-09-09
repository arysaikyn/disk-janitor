<script lang="ts">
  // 还我磁盘 — press button. 40px hit area; scale 0.96 on press; hover lifts per kind.
  // kind: primary = solid teal w/ ink text · ghost = transparent + hairline ring ·
  //       danger = red tint. Disabled = 0.4 opacity, no press scale.
  let {
    label,
    kind = "primary",
    disabled = false,
    onpress,
  }: {
    label: string;
    kind?: "primary" | "ghost" | "danger";
    disabled?: boolean;
    onpress?: () => void;
  } = $props();
</script>

<button
  class="press"
  class:primary={kind === "primary"}
  class:ghost={kind === "ghost"}
  class:danger={kind === "danger"}
  disabled={disabled}
  onclick={onpress}
>
  {label}
</button>

<style>
  .press {
    min-height: 40px;
    padding: 8px 18px;
    border-radius: var(--r-sm);
    font-weight: 500;
    letter-spacing: 0.01em;
    transition:
      background var(--t-fast) var(--ease),
      color var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
  }

  .press:disabled {
    opacity: 0.4;
    cursor: default;
    transform: none;
  }

  .primary {
    background: var(--accent);
    color: var(--ink);
    font-weight: 600;
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .primary:active:not(:disabled) {
    background: var(--accent-pressed);
    transform: scale(0.96);
  }

  .ghost {
    box-shadow: inset 0 0 0 1px var(--hairline);
  }
  .ghost:hover:not(:disabled) {
    background: var(--surface-lift);
  }
  .ghost:active:not(:disabled) {
    transform: scale(0.96);
  }

  .danger {
    background: rgba(248, 113, 113, 0.12);
    color: var(--danger);
  }
  .danger:hover:not(:disabled) {
    background: rgba(248, 113, 113, 0.2);
  }
  .danger:active:not(:disabled) {
    background: rgba(248, 113, 113, 0.26);
    transform: scale(0.96);
  }
</style>
