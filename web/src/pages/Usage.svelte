<script lang="ts">
  import type { DayCell, Dimension, Granularity, HeatCell } from '../lib/api/contract';
  import { DAY_KEYS } from '../lib/i18n/format';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import ChartCard from '../lib/charts/ChartCard.svelte';
  import ShareBars from '../lib/charts/ShareBars.svelte';
  import StackedBars, { type BarDatum } from '../lib/charts/StackedBars.svelte';
  import { modelColor, modelLabel, modelSeries } from '../lib/charts/series';
  import { fmt, i18n, t, tk } from '../lib/i18n/index.svelte';
  import { rangeFor, type DateRange } from '../lib/range';
  import { router } from '../lib/stores/router.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../lib/ui/DataTable.svelte';
  import DateRangePicker from '../lib/ui/DateRangePicker.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Select from '../lib/ui/Select.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import Tabs from '../lib/ui/Tabs.svelte';
  import HourHeatmap from '../lib/charts/HourHeatmap.svelte';
  import CalendarHeatmap from '../lib/charts/CalendarHeatmap.svelte';
  import { app } from '../lib/stores/app.svelte';

  let range = $state<DateRange>(rangeFor('last_30', i18n.now));
  let group = $state<Granularity>('day');
  let by = $state<Dimension>('project');
  const model = $derived(router.current.query.model);
  const params = $derived({ from: range.from, to: range.to, group, model });
  const usage = useQuery(() => queries.usage(params));
  const breakdown = useQuery(() => queries.breakdown({ from: range.from, to: range.to, by, model }));
  const heat = useQuery(() => queries.heatmap({ from: range.from, to: range.to, model }));
  const year = useQuery(() => queries.heatmap({}));

  const fold = $derived.by(() => {
    const totals = new Map<string, number>();
    for (const b of usage.data?.buckets ?? []) for (const s of b.by_model) totals.set(s.model, (totals.get(s.model) ?? 0) + s.cost_usd);
    return modelSeries(totals, t('common.other'), 4);
  });
  const data = $derived<BarDatum[]>(
    (usage.data?.buckets ?? []).map((b) => {
      const values: Record<string, number> = {};
      for (const s of b.by_model) values[fold.fold(s.model)] = (values[fold.fold(s.model)] ?? 0) + s.cost_usd;
      const label = group === 'hour' ? fmt.dateTime(b.start) : group === 'month' ? b.label : fmt.date(b.start, 'medium');
      return { key: b.label, label, tick: group === 'hour' ? fmt.time(b.start) : group === 'month' ? b.label : fmt.date(b.start, 'short'), values };
    }),
  );
  let heatSort = $state('cost');
  let daySort = $state('date');
  const heatColumns = $derived<Column<HeatCell>[]>([
    { key: 'day', label: t('chart.weekday'), value: (c) => c.dow },
    { key: 'hour', label: t('chart.hour'), value: (c) => c.hour },
    { key: 'cost', label: t('chart.cost'), align: 'end', value: (c) => c.cost_usd },
  ]);
  const dayColumns = $derived<Column<DayCell>[]>([
    { key: 'date', label: t('chart.date'), value: (d) => d.date },
    { key: 'cost', label: t('chart.cost'), align: 'end', value: (d) => d.cost_usd },
  ]);
  const dims: Dimension[] = ['project', 'model', 'agent', 'client', 'branch', 'tool', 'skill', 'mcp'];
  const columns = $derived<Column<BarDatum>[]>([
    { key: 'bucket', label: t('chart.date'), value: (r) => r.key },
    { key: 'total', label: t('common.total'), align: 'end', value: (r) => Object.values(r.values).reduce((a, b) => a + b, 0) },
  ]);
</script>

<PageHeader title={t('pages.usage.title')} lede={t('pages.usage.lede')}>
  {#snippet filters()}
    <DateRangePicker bind:value={range} />
    <Tabs
      label={t('pages.usage.over_time')}
      items={(['hour', 'day', 'week', 'month'] as const).map((g) => ({ id: g, label: t(`pages.usage.group.${g}`) }))}
      bind:value={group}
    />
    {#if model}<span class="chip">{modelLabel(model)}</span>{/if}
  {/snippet}
</PageHeader>

<div class="stack">
  {#if usage.data}
    <ChartCard title={t('pages.usage.over_time')} legend={fold.series} refreshing={usage.refreshing}>
      {#snippet note()}<ConfidenceBadge confidence={usage.data?.confidence ?? 'lower_bound'} />{/snippet}
      {#snippet chart()}
        <StackedBars {data} series={fold.series} label={t('pages.usage.over_time')} format={(v) => fmt.money(v)} axisFormat={(v) => fmt.money(v, { compact: true, digits: 0 })} totalLabel={t('common.total')} height={260} />
      {/snippet}
      {#snippet table()}
        <DataTable caption={t('pages.usage.over_time')} rows={data} {columns} rowKey={(r) => r.key} dense>
          {#snippet cell(row, col)}{col.key === 'bucket' ? row.label : fmt.money(Object.values(row.values).reduce((a, b) => a + b, 0))}{/snippet}
        </DataTable>
      {/snippet}
    </ChartCard>
  {:else}
    <Card><Skeleton height="260px" /></Card>
  {/if}

  <Card title={t('pages.usage.breakdown')} refreshing={breakdown.refreshing}>
    {#snippet actions()}
      <Select label={t('pages.usage.by')} hideLabel size="sm" bind:value={by} options={dims.map((d) => ({ value: d, label: tk(`pages.usage.dims.${d}`) }))} />
    {/snippet}
    {#if breakdown.data}
      <p class="conf"><ConfidenceBadge confidence={breakdown.data.confidence} /></p>
      <ShareBars
        label={t('pages.usage.breakdown')}
        shareFormat={(s) => fmt.ratio(s)}
        rows={breakdown.data.rows.slice(0, 12).map((r) => ({
          key: r.key,
          label: by === 'model' ? modelLabel(r.label) : r.label || t('common.none'),
          value: fmt.money(r.usage.cost_usd),
          share: r.share,
          color: by === 'model' ? modelColor(r.key) : undefined,
        }))}
      />
    {:else}
      <Skeleton lines={5} />
    {/if}
  </Card>
  <div class="two">
    {#if heat.data}
      {@const cells = heat.data.cells.filter((c) => c.cost_usd > 0)}
      <ChartCard title={t('pages.usage.when')} subtitle={t('pages.usage.when_sub')} refreshing={heat.refreshing}>
        {#snippet note()}<ConfidenceBadge confidence="lower_bound" />{/snippet}
        {#snippet chart()}
          <HourHeatmap cells={heat.data?.cells ?? []} label={t('pages.usage.when')} format={(v) => fmt.money(v)} weekStart={app.settings?.week_start ?? 0} />
        {/snippet}
        {#snippet table()}
          <DataTable caption={t('pages.usage.when')} rows={cells} columns={heatColumns} rowKey={(c) => `${c.dow}-${c.hour}`} bind:sortKey={heatSort} dense>
            {#snippet cell(c, col)}
              {#if col.key === 'day'}{tk(`chart.weekday_short.${DAY_KEYS[c.dow]}`)}{:else if col.key === 'hour'}{String(c.hour).padStart(2, '0')}:00{:else}{fmt.money(c.cost_usd)}{/if}
            {/snippet}
          </DataTable>
        {/snippet}
      </ChartCard>
    {:else}<Card><Skeleton height="160px" /></Card>{/if}
    {#if year.data}
      {@const days = year.data.days.filter((d) => d.cost_usd > 0)}
      <ChartCard title={t('pages.usage.year')} subtitle={t('pages.usage.year_sub')} refreshing={year.refreshing}>
        {#snippet note()}<ConfidenceBadge confidence="lower_bound" />{/snippet}
        {#snippet chart()}
          <CalendarHeatmap days={year.data?.days ?? []} label={t('pages.usage.year')} format={(v) => fmt.money(v)} weekStart={app.settings?.week_start ?? 0} />
        {/snippet}
        {#snippet table()}
          <DataTable caption={t('pages.usage.year')} rows={days} columns={dayColumns} rowKey={(d) => d.date} bind:sortKey={daySort} dense>
            {#snippet cell(d, col)}{col.key === 'date' ? d.date : fmt.money(d.cost_usd)}{/snippet}
          </DataTable>
        {/snippet}
      </ChartCard>
    {:else}<Card><Skeleton height="120px" /></Card>{/if}
  </div>
  <p class="preview">{t('pages.preview')}</p>
</div>

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .two {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 420px), 1fr));
    gap: var(--sp-5);
  }
  .conf {
    margin-bottom: var(--sp-3);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    min-height: 30px;
    padding: 0 var(--sp-3);
    border-radius: var(--r-pill);
    background: var(--accent-soft);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .preview {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
