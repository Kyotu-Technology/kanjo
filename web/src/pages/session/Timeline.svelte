<script lang="ts">
  import type { UsageBucket } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import StackedBars, { type BarDatum } from '../../lib/charts/StackedBars.svelte';
  import { modelSeries } from '../../lib/charts/series';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';

  interface Props {
    timeline: UsageBucket[];
    refreshing?: boolean;
  }
  let { timeline, refreshing = false }: Props = $props();
  const step = $derived(timeline.length > 1 ? timeline[1].start - timeline[0].start : 3_600_000);
  const kind = $derived(step < 3_600_000 ? '5m' : step < 86_400_000 ? 'hour' : 'day');
  const model = $derived.by(() => {
    const totals = new Map<string, number>();
    for (const b of timeline) for (const s of b.by_model) totals.set(s.model, (totals.get(s.model) ?? 0) + s.cost_usd);
    return modelSeries(totals, t('common.other'), 4);
  });
  const data = $derived<BarDatum[]>(
    timeline.map((b) => {
      const values: Record<string, number> = {};
      for (const s of b.by_model) {
        const k = model.fold(s.model);
        values[k] = (values[k] ?? 0) + s.cost_usd;
      }
      return kind === 'day'
        ? { key: b.label, label: fmt.date(b.start, 'long'), tick: fmt.date(b.start, 'short'), values }
        : { key: b.label, label: fmt.dateTime(b.start), tick: fmt.time(b.start), values };
    }),
  );
  const total = (r: BarDatum) => Object.values(r.values).reduce((a, b) => a + b, 0);
  const columns = $derived<Column<BarDatum>[]>([
    { key: 'time', label: t('pages.sessions.turn.when'), value: (r) => r.key },
    ...model.series.map((s) => ({ key: s.key, label: s.label, align: 'end' as const, optional: true, value: (r: BarDatum) => r.values[s.key] ?? 0 })),
    { key: 'total', label: t('common.total'), align: 'end', value: total },
  ]);
</script>

<ChartCard title={t('pages.session.timeline')} subtitle={t(`pages.session.timeline_${kind}`)} legend={model.series} {refreshing}>
  {#snippet note()}<span class="note"><ConfidenceBadge confidence="lower_bound" /></span>{/snippet}
  {#snippet chart()}
    <StackedBars {data} series={model.series} label={t('pages.session.timeline')} format={(v) => fmt.money(v)} axisFormat={(v) => fmt.money(v, { compact: true })} totalLabel={t('common.total')} height={220} />
  {/snippet}
  {#snippet table()}
    <DataTable caption={t('pages.session.timeline')} rows={data.filter((d) => total(d) > 0)} {columns} rowKey={(r) => r.key} dense>
      {#snippet cell(row, col)}
        {#if col.key === 'time'}{row.label}{:else if col.key === 'total'}<strong>{fmt.money(total(row))}</strong>{:else}{fmt.money(row.values[col.key] ?? 0)}{/if}
      {/snippet}
    </DataTable>
  {/snippet}
</ChartCard>

<style>
  .note {
    display: flex;
    justify-content: flex-end;
  }
</style>
