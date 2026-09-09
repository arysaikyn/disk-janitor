<script lang="ts">
  // 还我磁盘 — empty state: mini idle orb + one warm sentence + optional action slot.
  // title = the warm sentence; hint = secondary line; default slot = action (PressButton…).
  // Orb API contract (OrbCore): { size: number, state: "idle" } for the mini idle orb.
  import Orb from "./Orb.svelte";

  let {
    title,
    hint,
    children,
  }: {
    title: string;
    hint?: string;
    children?: import("svelte").Snippet;
  } = $props();
</script>

<div class="empty">
  <Orb size={48} state="idle" />
  <div class="title">{title}</div>
  {#if hint}
    <div class="hint">{hint}</div>
  {/if}
  {#if children}
    <div class="action">{@render children()}</div>
  {/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px 24px;
    text-align: center;
  }

  .title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }

  .hint {
    font-size: 13px;
    line-height: 1.5;
    color: var(--mist);
    max-width: 36ch;
  }

  .action {
    margin-top: 8px;
  }
</style>
