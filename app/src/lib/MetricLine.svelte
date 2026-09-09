<script lang="ts">
  // 还我磁盘 — metric line, replaces hero cards. Label + big tabular number + optional sub.
  // NO box, NO border: hierarchy from spacing alone. Callers separate metric groups with
  // a 32px gap (flex gap on the parent) — this component draws no chrome.
  // value is a pre-formatted string ("30.9 GB") — callers own number formatting.
  let {
    label,
    value,
    sub,
    small = false,
  }: { label: string; value: string; sub?: string; small?: boolean } = $props();
</script>

<div class="metric">
  <div class="label">{label}</div>
  <div class="value num" class:small>{value}</div>
  {#if sub}
    <div class="sub">{sub}</div>
  {/if}
</div>

<style>
  .metric {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label {
    font-size: 12px;
    color: var(--mist);
  }

  .value {
    font-family: var(--mono);
    font-size: 38px;
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.01em;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }

  /* Text-valued metrics (a state, not a number) render at label-adjacent scale —
     a sentence in the 38px slot reads as a layout bug. */
  .value.small {
    font-size: 15px;
    font-weight: 500;
    color: var(--mist);
    line-height: 1.5;
    letter-spacing: 0;
  }

  .sub {
    font-size: 12px;
    color: var(--mist);
  }
</style>
