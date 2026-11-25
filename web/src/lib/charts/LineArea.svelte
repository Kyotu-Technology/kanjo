<script lang="ts" module>
  export interface LinePoint {
    x: number;
    y: number;
  }
  export interface LineSeries {
    key: string;
    label: string;
    color: string;
    points: LinePoint[];
  }
</script>

<script lang="ts">
  import ChartTooltip from './ChartTooltip.svelte';
  import { linear, niceTicks } from './scale';

  interface Props {
    series: LineSeries[];
    label: string;
    format: (v: number) => string;
    xFormat: (x: number) => string;
    xTick?: (x: number) => string;
    /** Fixed top of the value axis, e.g. 100 for percentages. */
    yMax?: number;
    /** Solid reference line with a label, e.g. the limit at 100%. */
    reference?: { y: number; label: string };
    height?: number;
    area?: boolean;
  }
  let { series, label, format, xFormat, xTick = xFormat, yMax: fixedMax, reference, height = 220, area = true }: Props = $props();

  let width = $state(600);
  let hover = $state<number | null>(null);
  const M = { top: 10, right: 8, bottom: 24, left: 44 };

  const all = $derived(series.flatMap((s) => s.points));
  const xs = $derived([...new Set(all.map((p) => p.x))].sort((a, b) => a - b));
  const x0 = $derived(xs[0] ?? 0);
  const x1 = $derived(xs[xs.length - 1] ?? 1);
  const ticks = $derived(niceTicks(fixedMax ?? Math.max(...all.map((p) => p.y), reference?.y ?? 0, 0)));
  const yTop = $derived(fixedMax ?? ticks[ticks.length - 1]);
  const plotW = $derived(Math.max(10, width - M.left - M.right));
  const plotH = $derived(height - M.top - M.bottom);
  const sx = $derived(linear(x0, x1, M.left, M.left + plotW));
  const sy = $derived(linear(0, yTop, M.top + plotH, M.top));
  const xTicks = $derived.by(() => {
    const n = Math.max(2, Math.min(6, Math.floor(plotW / 90)));
    return Array.from({ length: n }, (_, i) => x0 + ((x1 - x0) * i) / (n - 1));
  });

  const paths = $derived(
    series.map((s) => {
      const d = s.points.map((p, i) => `${i ? 'L' : 'M'}${sx(p.x).toFixed(1)},${sy(p.y).toFixed(1)}`).join('');
      const a = s.points.length ? `${d}L${sx(s.points[s.points.length - 1].x)},${sy(0)}L${sx(s.points[0].x)},${sy(0)}Z` : '';
      return { key: s.key, color: s.color, d, a };
    }),
  );

  function nearest(px: number): number | null {
    if (!xs.length) return null;
    let best = 0;
    for (let i = 1; i < xs.length; i++) if (Math.abs(sx(xs[i]) - px) < Math.abs(sx(xs[best]) - px)) best = i;
    return best;
  }

  function onMove(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    hover = nearest(e.clientX - r.left);
  }

  function onKey(e: KeyboardEvent) {
    if (!xs.length) return;
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault();
      const cur = hover ?? (e.key === 'ArrowRight' ? -1 : xs.length);
      hover = Math.max(0, Math.min(xs.length - 1, cur + (e.key === 'ArrowRight' ? 1 : -1)));
    } else if (e.key === 'Escape') hover = null;
  }

  const tip = $derived.by(() => {
    if (hover === null) return null;
    const xv = xs[hover];
    const rows = series.flatMap((s) => {
      const p = s.points.find((q) => q.x === xv);
      return p ? [{ label: s.label, value: format(p.y), color: s.color }] : [];
    });
    const ys = series.map((s) => s.points.find((q) => q.x === xv)?.y ?? 0);
    return { title: xFormat(xv), rows, x: sx(xv), y: Math.min(sy(Math.max(...ys)) - 10, plotH - 40), xv };
  });
</script>

<div class="chart" bind:clientWidth={width}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <svg {width} {height} role="img" aria-label={label} tabindex="0" onpointermove={onMove} onpointerleave={() => (hover = null)} onkeydown={onKey} onblur={() => (hover = null)}>
    {#each ticks as tv (tv)}
      <line class="grid" class:base={tv === 0} x1={M.left} x2={M.left + plotW} y1={sy(tv)} y2={sy(tv)} />
      <text class="ytick" x={M.left - 8} y={sy(tv)} dy="0.32em" text-anchor="end">{format(tv)}</text>
    {/each}
    {#if reference}
      <line class="ref" x1={M.left} x2={M.left + plotW} y1={sy(reference.y)} y2={sy(reference.y)} />
      <text class="ref-label" x={M.left + plotW} y={sy(reference.y) - 6} text-anchor="end">{reference.label}</text>
    {/if}
    {#each paths as p (p.key)}
      {#if area && series.length === 1}<path d={p.a} fill={p.color} class="area" />{/if}
      <path d={p.d} stroke={p.color} class="line" />
    {/each}
    {#if tip}
      <line class="cross" x1={tip.x} x2={tip.x} y1={M.top} y2={M.top + plotH} />
      {#each series as s (s.key)}
        {@const pt = s.points.find((q) => q.x === tip.xv)}
        {#if pt}<circle cx={sx(pt.x)} cy={sy(pt.y)} r="4.5" fill={s.color} class="dot" />{/if}
      {/each}
    {/if}
    {#each xTicks as xt, i (i)}
      <text class="xtick" x={sx(xt)} y={height - 6} text-anchor={i === 0 ? 'start' : i === xTicks.length - 1 ? 'end' : 'middle'}>{xTick(xt)}</text>
    {/each}
  </svg>
  {#if tip}
    <ChartTooltip title={tip.title} rows={tip.rows} x={tip.x} y={Math.max(0, tip.y)} containerWidth={width} />
  {/if}
</div>

<style>
  .chart {
    position: relative;
    width: 100%;
    line-height: 0;
  }
  svg {
    display: block;
    overflow: visible;
  }
  svg:focus-visible {
    outline-offset: 4px;
  }
  .grid {
    stroke: var(--grid);
    shape-rendering: crispEdges;
  }
  .grid.base {
    stroke: var(--axis);
  }
  .ref {
    stroke: var(--ink-2);
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
  .ref-label {
    fill: var(--ink-2);
    font-weight: var(--fw-medium);
  }
  .line {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .area {
    opacity: 0.1;
  }
  .cross {
    stroke: var(--ink-3);
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
  .dot {
    stroke: var(--surface);
    stroke-width: 2;
  }
  text {
    fill: var(--ink-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
</style>
