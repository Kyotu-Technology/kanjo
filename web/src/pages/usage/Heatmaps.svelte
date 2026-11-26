<script lang="ts">
  import type { DayCell, HeatCell, Heatmap } from '../../lib/api/contract';
  import CalendarHeatmap from '../../lib/charts/CalendarHeatmap.svelte';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import HourHeatmap from '../../lib/charts/HourHeatmap.svelte';
  import { DAY_KEYS } from '../../lib/i18n/format';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';

  interface Props {
    heat: Heatmap | undefined;
    year: Heatmap | undefined;
    weekStart: number;
    heatRefreshing?: boolean;
    yearRefreshing?: boolean;
    only?: 'hour' | 'year';
  }
  let { heat, year, weekStart, heatRefreshing = false, yearRefreshing = false, only }: Props = $props();
  let heatSort = $state('cost');
  let heatDir = $state<'asc' | 'desc'>('desc');
  let daySort = $state('date');
  let dayDir = $state<'asc' | 'desc'>('desc');
  const hour = (h: number) => `${String(h).padStart(2, '0')}:00`;
  const dayTs = (d: DayCell) => {
    const [y, m, dd] = d.date.split('-').map(Number);
    return new Date(y, m - 1, dd).getTime();
  };
  const cells = $derived((heat?.cells ?? []).filter((c) => c.cost_usd > 0));
  const days = $derived((year?.days ?? []).filter((d) => d.cost_usd > 0));
  const busiestCell = $derived(cells.reduce<HeatCell | null>((m, c) => (!m || c.cost_usd > m.cost_usd ? c : m), null));
  const busiestDay = $derived(days.reduce<DayCell | null>((m, d) => (!m || d.cost_usd > m.cost_usd ? d : m), null));
  const heatColumns = $derived<Column<HeatCell>[]>([
    { key: 'day', label: t('chart.weekday'), value: (c) => (c.dow - weekStart + 7) % 7 },
    { key: 'hour', label: t('chart.hour'), value: (c) => c.hour },
    { key: 'requests', label: t('chart.requests'), align: 'end', optional: true, value: (c) => c.requests },
    { key: 'cost', label: t('chart.cost'), align: 'end', value: (c) => c.cost_usd },
  ]);
  const dayColumns = $derived<Column<DayCell>[]>([
    { key: 'date', label: t('chart.date'), value: (d) => d.date },
    { key: 'requests', label: t('chart.requests'), align: 'end', optional: true, value: (d) => d.requests },
    { key: 'cost', label: t('chart.cost'), align: 'end', value: (d) => d.cost_usd },
  ]);
</script>

<div class:two={!only}>
  {#if only === 'year'}{:else if heat}
    <ChartCard title={t('pages.usage.when')} subtitle={t('pages.usage.when_sub')} refreshing={heatRefreshing}>
      {#snippet note()}
        <span class="note">
          <span>{#if busiestCell}{t('pages.usage.busiest_hour', { day: tk(`chart.weekday_short.${DAY_KEYS[busiestCell.dow]}`), hour: hour(busiestCell.hour), value: fmt.money(busiestCell.cost_usd) })}{/if}</span>
          <ConfidenceBadge confidence="lower_bound" />
        </span>
      {/snippet}
      {#snippet chart()}
        <HourHeatmap cells={heat?.cells ?? []} label={t('pages.usage.when')} format={(v) => fmt.money(v)} {weekStart} />
      {/snippet}
      {#snippet table()}
        <DataTable caption={t('pages.usage.when')} rows={cells} columns={heatColumns} rowKey={(c) => `${c.dow}-${c.hour}`} bind:sortKey={heatSort} bind:sortDir={heatDir} dense>
          {#snippet cell(c, col)}
            {#if col.key === 'day'}{tk(`chart.weekday_short.${DAY_KEYS[c.dow]}`)}{:else if col.key === 'hour'}{hour(c.hour)}{:else if col.key === 'requests'}{fmt.number(c.requests)}{:else}{fmt.money(c.cost_usd)}{/if}
          {/snippet}
        </DataTable>
      {/snippet}
    </ChartCard>
  {:else}<Card><Skeleton height="180px" /></Card>{/if}
  {#if only === 'hour'}{:else if year}
    <ChartCard title={t('pages.usage.year')} subtitle={t('pages.usage.year_sub')} refreshing={yearRefreshing}>
      {#snippet note()}
        <span class="note">
          <span>{#if busiestDay}{t('pages.usage.busiest_day', { date: fmt.date(dayTs(busiestDay), 'long'), value: fmt.money(busiestDay.cost_usd) })}{/if}</span>
          <ConfidenceBadge confidence="lower_bound" />
        </span>
      {/snippet}
      {#snippet chart()}
        <CalendarHeatmap days={year?.days ?? []} label={t('pages.usage.year')} format={(v) => fmt.money(v)} {weekStart} />
      {/snippet}
      {#snippet table()}
        <DataTable caption={t('pages.usage.year')} rows={days} columns={dayColumns} rowKey={(d) => d.date} bind:sortKey={daySort} bind:sortDir={dayDir} dense>
          {#snippet cell(d, col)}
            {#if col.key === 'date'}{fmt.date(dayTs(d), 'long')}{:else if col.key === 'requests'}{fmt.number(d.requests)}{:else}{fmt.money(d.cost_usd)}{/if}
          {/snippet}
        </DataTable>
      {/snippet}
    </ChartCard>
  {:else}<Card><Skeleton height="140px" /></Card>{/if}
</div>

<style>
  .two {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 440px), 1fr));
    gap: var(--sp-5);
  }
  .note {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    flex-wrap: wrap;
  }
</style>
