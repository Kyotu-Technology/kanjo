<script lang="ts">
  import type { Confidence, UsageBucket } from '../../lib/api/contract';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import StackedBars, { type BarDatum } from '../../lib/charts/StackedBars.svelte';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';

  interface Props {
    hourly: UsageBucket[];
    confidence: Confidence;
    refreshing?: boolean;
  }
  let { hourly, confidence, refreshing = false }: Props = $props();
  const series = [{ key: 'cost', label: t('chart.cost'), color: 'var(--series-1)' }];
  const data = $derived<BarDatum[]>(
    hourly.map((b, i) => {
      const h = new Date(b.start).getHours();
      return {
        key: b.label,
        label: `${fmt.date(b.start, 'short')}, ${fmt.time(b.start)}`,
        tick: i === hourly.length - 1 ? t('dashboard.activity.now') : h % 6 === 0 ? `${String(h).padStart(2, '0')}:00` : null,
        values: { cost: b.usage.cost_usd },
      };
    }),
  );
  const total = $derived(hourly.reduce((a, b) => a + b.usage.cost_usd, 0));
  const peak = $derived(hourly.reduce<UsageBucket | null>((m, b) => (!m || b.usage.cost_usd > m.usage.cost_usd ? b : m), null));
  const columns = $derived<Column<BarDatum>[]>([
    { key: 'hour', label: t('chart.hour'), value: (r) => r.key },
    { key: 'cost', label: t('chart.cost'), align: 'end', value: (r) => r.values.cost },
  ]);
</script>

<ChartCard title={t('dashboard.activity.title')} subtitle={t('dashboard.activity.subtitle')} {refreshing}>
  {#snippet note()}
    <span class="note">
      {#if peak && peak.usage.cost_usd > 0}<span>{t('dashboard.activity.summary', { value: fmt.money(total), hour: fmt.time(peak.start) })}</span>{/if}
      <ConfidenceBadge {confidence} />
    </span>
  {/snippet}
  {#snippet chart()}
    <StackedBars
      {data}
      {series}
      label={t('dashboard.activity.title')}
      format={(v) => fmt.money(v)}
      axisFormat={(v) => fmt.money(v, { compact: true, digits: 0 })}
      height={200}
      marker={data.length - 1}
    />
  {/snippet}
  {#snippet table()}
    <DataTable caption={t('dashboard.activity.title')} rows={[...data].reverse()} {columns} rowKey={(r) => r.key} dense>
      {#snippet cell(row, col)}
        {#if col.key === 'hour'}{row.label}{:else}{fmt.money(row.values.cost)}{/if}
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
