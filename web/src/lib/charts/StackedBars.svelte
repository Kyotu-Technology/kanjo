<script lang="ts" module>
  export interface BarDatum {
    key: string;
    /** Tooltip and table title, e.g. "Tue 24 Nov". */
    label: string;
    /** Short axis label; null leaves the slot blank. */
    tick: string | null;
    values: Record<string, number>;
  }
</script>

<script lang="ts">
  import type { Series } from './series';
  import ChartTooltip from './ChartTooltip.svelte';
  import { linear, niceTicks, roundedTop } from './scale';

  interface Props {
    data: BarDatum[];
    series: Series[];
    label: string;
    format: (v: number) => string;
    axisFormat?: (v: number) => string;
    totalLabel?: string;
    height?: number;
    /** Index drawn with a hairline marker, e.g. the current hour. */
    marker?: number;
  }
  let { data, series, label, format, axisFormat = format, totalLabel = 'Total', height = 220, marker }: Props = $props();

  let width = $state(600);
  let active = $state<number | null>(null);
  const M = { top: 8, right: 4, bottom: 24, left: 48 };
  const GAP = 2;

  const totals = $derived(data.map((d) => series.reduce((a, s) => a + (d.values[s.key] ?? 0), 0)));
  const ticks = $derived(niceTicks(Math.max(...totals, 0)));
  const yMax = $derived(ticks[ticks.length - 1]);
  const plotW = $derived(Math.max(10, width - M.left - M.right));
  const plotH = $derived(height - M.top - M.bottom);
  const band = $derived(plotW / Math.max(1, data.length));
  const barW = $derived(Math.max(2, Math.min(24, band * 0.62)));
  const y = $derived(linear(0, yMax, M.top + plotH, M.top));
  const shownTicks = $derived.by(() => {
    const keep = new Set<number>();
    let lastX = Infinity;
    for (let i = data.length - 1; i >= 0; i--) {
      if (data[i].tick === null) continue;
      const x = M.left + i * band + band / 2;
      if (lastX - x >= 56) {
        keep.add(i);
        lastX = x;
      }
    }
    return keep;
  });

  const bars = $derived(
    data.map((d, i) => {
      const x = M.left + i * band + (band - barW) / 2;
      let base = M.top + plotH;
      const parts = series
        .map((s) => ({ s, v: d.values[s.key] ?? 0 }))
        .filter((p) => p.v > 0);
      const segs = parts.map((p, j) => {
        const h = (p.v / yMax) * plotH;
        const top = base - h;
        const drawH = j === 0 ? h : Math.max(0, h - GAP);
        const seg = { key: p.s.key, color: p.s.color, x, y: top, h: drawH, last: j === parts.length - 1 };
        base = top;
        return seg;
      });
      return { i, x, segs };
    }),
  );

  function onMove(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const i = Math.floor((e.clientX - r.left - M.left) / band);
    active = i >= 0 && i < data.length ? i : null;
  }

  function onKey(e: KeyboardEvent) {
    if (!data.length) return;
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault();
      const cur = active ?? (e.key === 'ArrowRight' ? -1 : data.length);
      active = Math.max(0, Math.min(data.length - 1, cur + (e.key === 'ArrowRight' ? 1 : -1)));
    } else if (e.key === 'Home') active = 0;
    else if (e.key === 'End') active = data.length - 1;
    else if (e.key === 'Escape') active = null;
  }

  const tip = $derived.by(() => {
    if (active === null || !data[active]) return null;
    const d = data[active];
    const rows = series
      .filter((s) => (d.values[s.key] ?? 0) > 0)
      .map((s) => ({ label: s.label, value: format(d.values[s.key] ?? 0), color: s.color }))
      .reverse();
    if (series.length > 1) rows.unshift({ label: totalLabel, value: format(totals[active]), color: '' });
    return { title: d.label, rows, x: M.left + active * band + band / 2, y: Math.min(y(totals[active]), plotH - 40) };
  });
</script>

<div class="chart" bind:clientWidth={width}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <svg
    {width}
    {height}
    role="img"
    aria-label={label}
    tabindex="0"
    onpointermove={onMove}
    onpointerleave={() => (active = null)}
    onkeydown={onKey}
    onblur={() => (active = null)}
  >
    {#each ticks as tv (tv)}
      <line class="grid" class:base={tv === 0} x1={M.left} x2={M.left + plotW} y1={y(tv)} y2={y(tv)} />
      <text class="ytick" x={M.left - 8} y={y(tv)} dy="0.32em" text-anchor="end">{axisFormat(tv)}</text>
    {/each}
    {#if marker !== undefined && marker >= 0 && marker < data.length}
      <line class="marker" x1={M.left + marker * band + band / 2} x2={M.left + marker * band + band / 2} y1={M.top} y2={M.top + plotH} />
    {/if}
    {#each bars as b (b.i)}
      <g class="bar" class:dim={active !== null && active !== b.i}>
        {#each b.segs as s (s.key)}
          {#if s.last}
            <path d={roundedTop(s.x, s.y, barW, s.h)} fill={s.color} />
          {:else}
            <rect x={s.x} y={s.y} width={barW} height={s.h} fill={s.color} />
          {/if}
        {/each}
      </g>
      {#if shownTicks.has(b.i)}
        <text class="xtick" x={b.x + barW / 2} y={height - 6} text-anchor="middle">{data[b.i].tick}</text>
      {/if}
    {/each}
  </svg>
  {#if tip}
    <ChartTooltip title={tip.title} rows={tip.rows} x={tip.x} y={tip.y} containerWidth={width} />
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
    border-radius: var(--r-xs);
  }
  svg:focus-visible {
    outline-offset: 4px;
  }
  .grid {
    stroke: var(--grid);
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
  .grid.base {
    stroke: var(--axis);
  }
  .marker {
    stroke: var(--ink-3);
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
  text {
    fill: var(--ink-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    transition: opacity var(--dur-fast) var(--ease);
  }
  .bar.dim {
    opacity: 0.45;
  }
</style>
