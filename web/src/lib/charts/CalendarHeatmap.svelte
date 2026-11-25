<script lang="ts">
  import type { DayCell } from '../api/contract';
  import { fmt, t } from '../i18n/index.svelte';
  import { seqStep } from './scale';

  interface Props {
    days: DayCell[];
    label: string;
    format: (v: number) => string;
    /** 0 = Monday. */
    weekStart?: number;
  }
  let { days, label, format, weekStart = 0 }: Props = $props();
  let active = $state<DayCell | null>(null);
  let width = $state(700);

  const parsed = $derived(
    days.map((d) => {
      const [y, m, dd] = d.date.split('-').map(Number);
      const ts = new Date(y, m - 1, dd).getTime();
      return { ...d, ts, dow: (new Date(ts).getDay() + 6) % 7 };
    }),
  );
  const max = $derived(Math.max(...days.map((d) => d.cost_usd), 0));
  const columns = $derived.by(() => {
    const cols: (typeof parsed)[] = [];
    let col: typeof parsed = [];
    for (const d of parsed) {
      const row = (d.dow - weekStart + 7) % 7;
      if (row === 0 && col.length) {
        cols.push(col);
        col = [];
      }
      col.push(d);
    }
    if (col.length) cols.push(col);
    return cols;
  });
  const lead = $derived(parsed.length ? (parsed[0].dow - weekStart + 7) % 7 : 0);
  const fit = $derived(Math.max(1, Math.floor((width + 3) / 14)));
  const visible = $derived(columns.slice(-fit));
  const offset = $derived(columns.length - visible.length);
</script>

<div class="cal">
  <div class="weeks" role="img" aria-label={label} bind:clientWidth={width}>
    {#each visible as col, ci (ci + offset)}
      <div class="week">
        {#if ci + offset === 0}{#each Array.from({ length: lead }, (_, i) => i) as i (i)}<span class="pad"></span>{/each}{/if}
        {#each col as d (d.date)}
          <span class="cell s{seqStep(d.cost_usd, max)}" class:on={active?.date === d.date} onpointerenter={() => (active = d)} onpointerleave={() => (active = null)} aria-hidden="true"></span>
        {/each}
      </div>
    {/each}
  </div>
  <p class="readout" aria-live="polite">{active ? `${fmt.date(parsed.find((p) => p.date === active?.date)?.ts ?? 0, 'long')}: ${format(active.cost_usd)}` : ''}</p>
  <div class="scale" aria-hidden="true">
    <span>{t('chart.less')}</span>
    {#each [1, 2, 3, 4, 5, 6] as s (s)}<span class="cell s{s}"></span>{/each}
    <span>{t('chart.more')}</span>
  </div>
</div>

<style>
  .weeks {
    display: flex;
    gap: 3px;
    overflow: hidden;
    justify-content: flex-end;
  }
  .week {
    display: grid;
    grid-template-rows: repeat(7, 11px);
    gap: 3px;
  }
  .cell,
  .pad {
    width: 11px;
    height: 11px;
    border-radius: 2px;
  }
  .cell {
    background: var(--seq-0);
  }
  .cell.on {
    outline: 2px solid var(--ink);
  }
  .s1 {
    background: var(--seq-1);
  }
  .s2 {
    background: var(--seq-2);
  }
  .s3 {
    background: var(--seq-3);
  }
  .s4 {
    background: var(--seq-4);
  }
  .s5 {
    background: var(--seq-5);
  }
  .s6 {
    background: var(--seq-6);
  }
  .readout {
    min-height: 20px;
    margin-top: var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .scale {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--ink-3);
  }
</style>
