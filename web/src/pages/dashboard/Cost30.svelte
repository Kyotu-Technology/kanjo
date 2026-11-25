<script lang="ts">
  import type { Confidence, UsageBucket } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import StackedBars, { type BarDatum } from '../../lib/charts/StackedBars.svelte';
  import { modelSeries } from '../../lib/charts/series';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';

  interface Props {
    daily: UsageBucket[];
    confidence: Confidence;
    refreshing?: boolean;
  }
  let { daily, confidence, refreshing = false }: Props = $props();

  const dayTs = (b: UsageBucket) => {
    const [y, m, d] = b.label.split('-').map(Number);
    return new Date(y, m - 1, d).getTime();
  };
  const model = $derived.by(() => {
    const totals = new Map<string, number>();
    for (const b of daily) for (const s of b.by_model) totals.set(s.model, (totals.get(s.model) ?? 0) + s.cost_usd);
    return modelSeries(totals, t('common.other'), 4);
  });
  const data = $derived<BarDatum[]>(
    daily.map((b, i) => {
      const values: Record<string, number> = {};
      for (const s of b.by_model) {
        const k = model.fold(s.model);
        values[k] = (values[k] ?? 0) + s.cost_usd;
      }
      const ts = dayTs(b);
      return { key: b.label, label: fmt.date(ts, 'long'), tick: i === daily.length - 1 ? t('time.today') : fmt.date(ts, 'short'), values };
    }),
  );
  const peak = $derived(daily.reduce<UsageBucket | null>((m, b) => (!m || b.usage.cost_usd > m.usage.cost_usd ? b : m), null));
  const columns = $derived<Column<BarDatum>[]>([
    { key: 'date', label: t('chart.date'), value: (r) => r.key },
    ...model.series.map((s) => ({ key: s.key, label: s.label, align: 'end' as const, optional: true, value: (r: BarDatum) => r.values[s.key] ?? 0 })),
    { key: 'total', label: t('common.total'), align: 'end', value: (r) => Object.values(r.values).reduce((a, b) => a + b, 0) },
  ]);
</script>

<ChartCard title={t('dashboard.cost30.title')} subtitle={t('dashboard.cost30.subtitle')} legend={model.series} {refreshing}>
  {#snippet note()}
    <span class="note">
      {#if peak && peak.usage.cost_usd > 0}{t('dashboard.cost30.peak', { value: fmt.money(peak.usage.cost_usd), date: fmt.date(dayTs(peak), 'medium') })}{/if}
      <ConfidenceBadge {confidence} />
    </span>
  {/snippet}
  {#snippet chart()}
    <StackedBars
      {data}
      series={model.series}
      label={t('dashboard.cost30.title')}
      format={(v) => fmt.money(v)}
      axisFormat={(v) => fmt.money(v, { compact: true, digits: 0 })}
      totalLabel={t('common.total')}
      height={240}
    />
  {/snippet}
  {#snippet table()}
    <DataTable caption={t('dashboard.cost30.title')} rows={[...data].reverse()} {columns} rowKey={(r) => r.key} dense>
      {#snippet cell(row, col)}
        {#if col.key === 'date'}{row.label}{:else if col.key === 'total'}<strong>{fmt.money(Object.values(row.values).reduce((a, b) => a + b, 0))}</strong>{:else}{fmt.money(row.values[col.key] ?? 0)}{/if}
      {/snippet}
    </DataTable>
  {/snippet}
</ChartCard>

<style>
  .note {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    flex-wrap: wrap;
  }
</style>
