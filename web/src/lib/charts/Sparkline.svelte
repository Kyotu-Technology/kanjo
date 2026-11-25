<script lang="ts">
  interface Props {
    values: number[];
    label: string;
    height?: number;
  }
  let { values, label, height = 32 }: Props = $props();
  let width = $state(160);
  const max = $derived(Math.max(...values, 1e-9));
  const pts = $derived(values.map((v, i) => [values.length > 1 ? (i / (values.length - 1)) * (width - 6) + 3 : 3, height - 3 - (v / max) * (height - 8)] as const));
  const line = $derived(pts.map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)},${y.toFixed(1)}`).join(''));
  const area = $derived(pts.length ? `${line}L${pts[pts.length - 1][0]},${height}L${pts[0][0]},${height}Z` : '');
  const last = $derived(pts[pts.length - 1]);
</script>

<div class="spark" bind:clientWidth={width}>
  <svg {width} {height} role="img" aria-label={label}>
    <path d={area} class="area" />
    <path d={line} class="line" />
    {#if last}<circle cx={last[0]} cy={last[1]} r="3.5" class="dot" />{/if}
  </svg>
</div>

<style>
  .spark {
    width: 100%;
    line-height: 0;
  }
  svg {
    overflow: visible;
  }
  .line {
    fill: none;
    stroke: var(--ink-3);
    stroke-width: 1.5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .area {
    fill: var(--ink-3);
    opacity: 0.08;
  }
  .dot {
    fill: var(--accent);
    stroke: var(--surface);
    stroke-width: 2;
  }
</style>
