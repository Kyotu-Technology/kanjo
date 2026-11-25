<script lang="ts">
  interface Row {
    label: string;
    value: string;
    color?: string;
  }
  interface Props {
    title: string;
    rows: Row[];
    /** Anchor in px inside the chart container. */
    x: number;
    y: number;
    containerWidth: number;
  }
  let { title, rows, x, y, containerWidth }: Props = $props();
  let el: HTMLDivElement | undefined = $state();
  const left = $derived.by(() => {
    const w = el?.offsetWidth ?? 180;
    const pos = x + 14;
    return pos + w > containerWidth ? Math.max(0, x - w - 14) : pos;
  });
</script>

<div bind:this={el} class="tt" style="left:{left}px;top:{Math.max(0, y)}px" aria-hidden="true">
  <p class="title">{title}</p>
  {#each rows as r (r.label)}
    <p class="row">
      {#if r.color}<span class="key" style="background:{r.color}"></span>{/if}
      <span class="v">{r.value}</span>
      <span class="l">{r.label}</span>
    </p>
  {/each}
</div>

<style>
  .tt {
    position: absolute;
    z-index: 5;
    pointer-events: none;
    min-width: 140px;
    padding: var(--sp-2) var(--sp-3);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--surface);
    box-shadow: var(--shadow-overlay);
    font-size: var(--fs-sm);
    line-height: 1.45;
    white-space: nowrap;
  }
  .title {
    color: var(--ink-3);
    font-size: var(--fs-xs);
    margin-bottom: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .key {
    width: 12px;
    height: 2px;
    border-radius: 1px;
    flex: none;
  }
  .v {
    font-weight: var(--fw-semibold);
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }
  .l {
    color: var(--ink-2);
  }
</style>
