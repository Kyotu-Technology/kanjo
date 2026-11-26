<script lang="ts">
  import type { Block } from '../../lib/api/contract';
  import ChartTooltip from '../../lib/charts/ChartTooltip.svelte';
  import { dayKey } from '../../lib/i18n/format';
  import { fmt, i18n, t, tk } from '../../lib/i18n/index.svelte';

  interface Props {
    blocks: Block[];
    days?: number;
  }
  let { blocks, days = 7 }: Props = $props();
  const HOUR = 3_600_000;

  function dayStart(offset: number): number {
    const d = new Date(i18n.now);
    d.setHours(0, 0, 0, 0);
    d.setDate(d.getDate() - offset);
    return d.getTime();
  }

  const max = $derived(Math.max(...blocks.map((b) => b.usage.cost_usd), 1e-9));
  const level = (b: Block) => Math.max(1, Math.min(5, Math.ceil((b.usage.cost_usd / max) * 5)));
  const dayLabel = (ts: number) => `${tk(`chart.weekday_short.${dayKey(ts)}`)} ${fmt.date(ts, 'short')}`;
  const kindLabel = (b: Block) => (b.anchored ? t('pages.limits.timeline.kind_anchored') : t('pages.limits.timeline.kind_inferred'));
  const describe = (b: Block) =>
    [
      t('pages.limits.timeline.window', { start: `${dayLabel(b.start)} ${fmt.time(b.start)}`, end: fmt.time(b.end) }),
      kindLabel(b),
      b.peak_pct === null ? t('pages.limits.timeline.no_peak') : `${t('pages.limits.peak')} ${fmt.pct(b.peak_pct)}`,
      fmt.money(b.usage.cost_usd),
    ].join(', ');

  interface Seg {
    key: string;
    block: Block;
    left: number;
    width: number;
    cutL: boolean;
    cutR: boolean;
    future: boolean;
    first: boolean;
  }

  const rows = $derived.by(() =>
    Array.from({ length: days }, (_, i) => {
      const start = dayStart(i);
      const end = dayStart(i - 1);
      const len = end - start;
      const segs: Seg[] = [];
      for (const b of blocks) {
        if (b.end <= start || b.start >= end) continue;
        const parts: [number, number, boolean][] = b.is_active ? [[b.start, i18n.now, false], [i18n.now, b.end, true]] : [[b.start, b.end, false]];
        for (const [a, z, future] of parts) {
          const from = Math.max(a, start);
          const to = Math.min(z, end);
          if (to <= from) continue;
          segs.push({
            key: `${b.start}-${future ? 'f' : 'p'}`,
            block: b,
            left: ((from - start) / len) * 100,
            width: ((to - from) / len) * 100,
            cutL: from > a || a < start,
            cutR: to < z,
            future,
            first: !future && from === Math.max(b.start, start) && b.start >= start,
          });
        }
      }
      return { start, wd: tk(`chart.weekday_short.${dayKey(start)}`), date: fmt.date(start, 'short'), segs, now: i === 0 ? ((i18n.now - start) / len) * 100 : null };
    }),
  );

  let host: HTMLDivElement | undefined = $state();
  let width = $state(600);
  let active = $state.raw<{ b: Block; x: number; y: number } | null>(null);

  function show(e: Event, b: Block) {
    if (!host) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const h = host.getBoundingClientRect();
    active = { b, x: r.left - h.left + r.width / 2, y: r.bottom - h.top + 4 };
  }

  const tip = $derived(
    active
      ? {
          title: t('pages.limits.timeline.window', { start: `${dayLabel(active.b.start)} ${fmt.time(active.b.start)}`, end: fmt.time(active.b.end) }),
          rows: [
            { label: t('pages.limits.timeline.col_kind'), value: active.b.is_active ? `${kindLabel(active.b)}, ${t('pages.limits.timeline.running').toLowerCase()}` : kindLabel(active.b) },
            { label: t('pages.limits.peak'), value: active.b.peak_pct === null ? t('pages.limits.timeline.no_peak') : fmt.pct(active.b.peak_pct) },
            { label: t('pages.limits.cost'), value: fmt.money(active.b.usage.cost_usd) },
            { label: t('pages.limits.timeline.col_requests'), value: fmt.number(active.b.usage.requests) },
          ],
        }
      : null,
  );
  const ticks = [0, 6, 12, 18];
</script>

<div class="timeline" bind:this={host} bind:clientWidth={width} role="group" aria-label={t('pages.limits.timeline.label')}>
  <div class="axis" aria-hidden="true">
    <span class="day"></span>
    <span class="hours">
      {#each ticks as h (h)}<span class="tick" style="left:{(h / 24) * 100}%">{fmt.time(dayStart(0) + h * HOUR)}</span>{/each}
    </span>
  </div>
  {#each rows as row (row.start)}
    <div class="row">
      <span class="day">{row.wd} <span class="date">{row.date}</span></span>
      <span class="track">
        {#each ticks as h (h)}<span class="grid" style="left:{(h / 24) * 100}%" aria-hidden="true"></span>{/each}
        {#each row.segs as s (s.key)}
          {#if s.future}
            <span class="seg future" class:cutL={s.cutL} class:cutR={s.cutR} style="left:{s.left}%;width:{s.width}%" aria-hidden="true"></span>
          {:else}
            <button
              type="button"
              class="seg l{level(s.block)}"
              class:inferred={!s.block.anchored}
              class:cutL={s.cutL}
              class:cutR={s.cutR}
              class:on={active?.b === s.block}
              style="left:{s.left}%;width:{s.width}%"
              tabindex={s.first ? 0 : -1}
              aria-label={describe(s.block)}
              onpointerenter={(e) => show(e, s.block)}
              onpointerleave={() => (active = null)}
              onfocus={(e) => show(e, s.block)}
              onblur={() => (active = null)}
            ></button>
          {/if}
        {/each}
        {#if row.now !== null}<span class="now" style="left:{row.now}%" title={t('pages.limits.timeline.now')}></span>{/if}
      </span>
    </div>
  {/each}
  {#if tip && active}
    <ChartTooltip title={tip.title} rows={tip.rows} x={active.x} y={active.y} containerWidth={width} />
  {/if}
</div>
<div class="legend">
  <span class="item"><span class="sw l4" aria-hidden="true"></span>{t('pages.limits.timeline.anchored')}</span>
  <span class="item"><span class="sw l4 inferred" aria-hidden="true"></span>{t('pages.limits.timeline.inferred')}</span>
  <span class="item ramp">
    {t('pages.limits.timeline.cheaper')}
    {#each [1, 2, 3, 4, 5] as l (l)}<span class="rs l{l}" aria-hidden="true"></span>{/each}
    {t('pages.limits.timeline.dearer')}
  </span>
</div>

<style>
  .timeline {
    position: relative;
    --day-w: 92px;
  }
  .axis,
  .row {
    display: grid;
    grid-template-columns: var(--day-w) minmax(0, 1fr);
    align-items: center;
  }
  .axis {
    height: 20px;
  }
  .hours,
  .track {
    position: relative;
    height: 100%;
  }
  .tick {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    color: var(--ink-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .tick:first-child {
    transform: none;
  }
  .row {
    height: 34px;
    border-top: 1px solid var(--line);
  }
  .day {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }
  .grid {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--grid);
  }
  .seg {
    position: absolute;
    top: 7px;
    bottom: 7px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    cursor: default;
    box-shadow: 0 0 0 1px var(--surface);
  }
  .seg.cutL {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
  }
  .seg.cutR {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .l1 {
    background-color: var(--seq-2);
  }
  .l2 {
    background-color: var(--seq-3);
  }
  .l3 {
    background-color: var(--seq-4);
  }
  .l4 {
    background-color: var(--seq-5);
  }
  .l5 {
    background-color: var(--seq-6);
  }
  .inferred {
    background-image: repeating-linear-gradient(135deg, transparent 0 4px, var(--surface) 4px 6.5px);
    outline: 1px dashed var(--ink-3);
    outline-offset: -1px;
  }
  .seg.on {
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 3px var(--ink);
  }
  .seg:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .future {
    background: none;
    border: 1px dashed var(--line-strong);
    box-shadow: none;
  }
  .now {
    position: absolute;
    top: 2px;
    bottom: 2px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--ink);
    box-shadow: 0 0 0 2px var(--surface);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2) var(--sp-5);
    margin-top: var(--sp-4);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .sw {
    width: 22px;
    height: 12px;
    border-radius: 3px;
  }
  .ramp {
    gap: 3px;
  }
  .rs {
    width: 14px;
    height: 12px;
    border-radius: 2px;
  }
  .rs:first-of-type {
    margin-left: var(--sp-1);
  }
  .rs:last-of-type {
    margin-right: var(--sp-1);
  }
  @media (max-width: 640px) {
    .timeline {
      --day-w: 40px;
    }
    .day {
      font-size: var(--fs-xs);
    }
    .date {
      display: none;
    }
  }
</style>
