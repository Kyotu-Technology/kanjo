<script lang="ts">
  import type { HeatCell } from '../api/contract';
  import { DAY_KEYS } from '../i18n/format';
  import { t } from '../i18n/index.svelte';
  import { seqStep } from './scale';

  interface Props {
    cells: HeatCell[];
    label: string;
    format: (v: number) => string;
    /** 0 = Monday. */
    weekStart?: number;
  }
  let { cells, label, format, weekStart = 0 }: Props = $props();
  let active = $state<{ dow: number; hour: number } | null>(null);
  const rows = $derived(Array.from({ length: 7 }, (_, i) => (i + weekStart) % 7));
  const byKey = $derived(new Map(cells.map((c) => [`${c.dow}-${c.hour}`, c])));
  const max = $derived(Math.max(...cells.map((c) => c.cost_usd), 0));
  const hourLabel = (h: number) => `${String(h).padStart(2, '0')}:00`;
  const activeText = $derived.by(() => {
    if (!active) return '';
    const c = byKey.get(`${active.dow}-${active.hour}`);
    return t('chart.heat_cell', { day: t(`chart.weekday_short.${DAY_KEYS[active.dow]}`), hour: hourLabel(active.hour), value: format(c?.cost_usd ?? 0) });
  });
</script>

<div class="heat">
  <div class="grid" role="img" aria-label={label}>
    {#each rows as dow (dow)}
      <span class="day">{t(`chart.weekday_short.${DAY_KEYS[dow]}`)}</span>
      {#each Array.from({ length: 24 }, (_, h) => h) as hour (hour)}
        {@const c = byKey.get(`${dow}-${hour}`)}
        <span
          class="cell s{seqStep(c?.cost_usd ?? 0, max)}"
          class:on={active?.dow === dow && active?.hour === hour}
          onpointerenter={() => (active = { dow, hour })}
          onpointerleave={() => (active = null)}
          aria-hidden="true"
        ></span>
      {/each}
    {/each}
    <span></span>
    {#each [0, 6, 12, 18] as h (h)}
      <span class="hour" style="grid-column:{h + 2} / span 6">{hourLabel(h)}</span>
    {/each}
  </div>
  <div class="foot">
    <p class="readout" aria-live="polite">{activeText}</p>
    <div class="scale" aria-hidden="true">
      <span>{t('chart.less')}</span>
      {#each [1, 2, 3, 4, 5, 6] as s (s)}<span class="cell s{s}"></span>{/each}
      <span>{t('chart.more')}</span>
    </div>
  </div>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: 3.2em repeat(24, minmax(0, 1fr));
    gap: 2px;
    align-items: center;
  }
  .day,
  .hour {
    font-size: 11px;
    color: var(--ink-3);
    line-height: 1;
  }
  .hour {
    padding-top: 4px;
  }
  .cell {
    display: block;
    aspect-ratio: 1;
    min-height: 8px;
    border-radius: 2px;
    background: var(--seq-0);
  }
  .cell.on {
    outline: 2px solid var(--ink);
    outline-offset: 0;
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
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--sp-3);
    margin-top: var(--sp-3);
    min-height: 20px;
  }
  .readout {
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
  .scale .cell {
    width: 10px;
    min-height: 10px;
  }
</style>
