<script lang="ts">
  // 还我磁盘 — interactive list row. No accent borders: hover = bg lift; selected =
  // teal-8% bg tint + 3px teal dot before content; press = scale 0.99.
  // grade = CSS var name WITHOUT the --grade- prefix ("grade-pure"); omit → no dot.
  // index → stagger enter 40ms/row, capped at 8. Default slot = row content
  // (title line, meta, actions). Rows are full-width and adjacent — callers place
  // them in a <ul> with no gaps; hairline separators: border-top per row,
  // first-child exempt (Svelte drops sibling selectors that can't match).
  let {
    selected = false,
    onclick,
    grade,
    index = 0,
    children,
  }: {
    selected?: boolean;
    onclick?: () => void;
    grade?: string;
    index?: number;
    children?: import("svelte").Snippet;
  } = $props();

  const stagger = $derived(`${Math.min(index, 8) * 40}ms`);
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role —
  Row is a composite interactive list item: it must stay an <li> (list semantics)
  and cannot be a real <button> because slot content may nest interactive
  elements (actions). role=button + tabindex + Enter/Space = keyboard operable. -->
<li
  class="row"
  class:selected
  style={`--stagger: ${stagger}`}
  role="button"
  tabindex="0"
  aria-pressed={selected}
  onclick={onclick}
  onkeydown={(e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onclick?.();
    }
  }}
>
  {#if grade}
    <span class="dot" style={`background: var(--${grade})`} aria-hidden="true"></span>
  {/if}
  <span class="sel-dot" aria-hidden="true"></span>
  <div class="body">{@render children?.()}</div>
</li>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    list-style: none;
    padding: 10px 12px;
    cursor: pointer;
    background: transparent;
    border-top: 1px solid var(--hairline);
    transition:
      opacity var(--t-med) var(--ease) var(--stagger, 0ms),
      background var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
  }
  .row:first-child {
    border-top: none;
  }
  /* entry: fade in (transform kept free for the press scale) */
  @starting-style {
    .row {
      opacity: 0;
    }
  }

  .row:hover {
    background: var(--surface-lift);
  }
  .row:active {
    transform: scale(0.99);
  }

  .row.selected {
    background: rgba(45, 212, 191, 0.08);
  }
  .row.selected:hover {
    background: rgba(45, 212, 191, 0.1);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }

  .sel-dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--accent);
    flex: none;
    opacity: 0;
    transform: scale(0);
    transition:
      opacity var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
  }
  .row.selected .sel-dot {
    opacity: 1;
    transform: scale(1);
  }

  .body {
    flex: 1;
    min-width: 0;
  }

  /* reduced motion: entry delay collapses with the animation */
  @media (prefers-reduced-motion: reduce) {
    .row {
      transition-delay: 0ms !important;
    }
  }
</style>
