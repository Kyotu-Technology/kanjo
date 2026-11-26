<script lang="ts">
  import type { Granularity, Tokens, UsageReport } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import StackedBars, { type BarDatum } from '../../lib/charts/StackedBars.svelte';
  import { modelSeries, type Series } from '../../lib/charts/series';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import Tabs from '../../lib/ui/Tabs.svelte';
  import { bucketText } from './labels';

  interface Props {
    report: UsageReport;
    group: Granularity;
    days: number;
    refreshing?: boolean;
  }
  let { report, group, days, refreshing = false }: Props = $props();
  let view = $state<'cost' | 'tokens'>('cost');

  const TOKEN_KEYS = ['input', 'output', 'cache_read', 'cache_write_5m', 'cache_write_1h'] as const satisfies readonly (keyof Tokens)[];

  const models = $derived.by(() => {
    const totals = new Map<string, number>();
    for (const b of report.buckets) for (const s of b.by_model) totals.set(s.model, (totals.get(s.model) ?? 0) + s.cost_usd);
    return modelSeries(totals, t('common.other'), 4);
  });
  const tokenSeries = $derived<Series[]>(TOKEN_KEYS.map((k, i) => ({ key: k, label: tk(`pages.usage.tokens.${k}`), color: `var(--series-${i + 1})` })));
  const series = $derived(view === 'cost' ? models.series : tokenSeries);

  const data = $derived<BarDatum[]>(
    report.buckets.map((b) => {
      const values: Record<string, number> = {};
      if (view === 'cost') {
        for (const s of b.by_model) {
          const k = models.fold(s.model);
          values[k] = (values[k] ?? 0) + s.cost_usd;
        }
      } else for (const k of TOKEN_KEYS) values[k] = b.usage.tokens[k];
      return { key: b.label, ...bucketText(b, group), values };
    }),
  );
  const format = $derived(view === 'cost' ? (v: number) => fmt.money(v) : (v: number) => t('units.tokens', { value: fmt.compact(v) }));
  const axis = $derived(view === 'cost' ? (v: number) => fmt.money(v, { compact: true, digits: 0 }) : (v: number) => fmt.compact(v));
  const total = (r: BarDatum) => Object.values(r.values).reduce((a, b) => a + b, 0);
  const peak = $derived(data.reduce<BarDatum | null>((m, d) => (!m || total(d) > total(m) ? d : m), null));
  const tokenTotal = $derived(TOKEN_KEYS.reduce((a, k) => a + report.total.tokens[k], 0));
  const columns = $derived<Column<BarDatum>[]>([
    { key: 'bucket', label: t('chart.date'), value: (r) => r.key },
    ...series.map((s) => ({ key: s.key, label: s.label, align: 'end' as const, optional: true, value: (r: BarDatum) => r.values[s.key] ?? 0 })),
    { key: 'total', label: t('common.total'), align: 'end', value: total },
  ]);
  const tableFormat = $derived(view === 'cost' ? (v: number) => fmt.money(v) : (v: number) => fmt.number(v));
</script>

<ChartCard
  title={view === 'cost' ? t('pages.usage.over_time') : t('pages.usage.tokens_over_time')}
  subtitle={view === 'cost' ? t('pages.usage.sub_cost') : t('pages.usage.sub_tokens')}
  legend={series}
  {refreshing}
>
  {#snippet note()}
    <div class="switch">
      <Tabs
        size="sm"
        label={t('pages.usage.view_label')}
        items={[
          { id: 'cost', label: t('pages.usage.view_cost') },
          { id: 'tokens', label: t('pages.usage.view_tokens') },
        ]}
        bind:value={view}
      />
    </div>
    <span class="note">
      <span>
        {#if view === 'tokens'}{t('pages.usage.tokens.note')}{:else if peak && total(peak) > 0}{t('pages.usage.peak', { value: format(total(peak)), date: peak.label })}{/if}
        {#if group === 'hour' && days > 7}<span class="warn">{t('pages.usage.hourly_dense')}</span>{/if}
      </span>
      {#if view === 'cost'}<ConfidenceBadge confidence={report.confidence} />{/if}
    </span>
  {/snippet}
  {#snippet chart()}
    <StackedBars {data} {series} label={t('pages.usage.over_time')} {format} axisFormat={axis} totalLabel={t('common.total')} height={260} />
    {#if view === 'tokens'}
      <dl class="classes">
        {#each tokenSeries as s (s.key)}
          {@const v = report.total.tokens[s.key as (typeof TOKEN_KEYS)[number]]}
          <div class="cls">
            <dt>
              <span class="key" style="background:{s.color}"></span>{s.label}
              <InlineHelp text={tk(`pages.usage.tokens.${s.key}_help`)} label={s.label} />
            </dt>
            <dd>
              <strong>{fmt.compact(v)}</strong>
              <span>{t('pages.usage.tokens.share', { share: fmt.ratio(tokenTotal > 0 ? v / tokenTotal : 0, v > 0 && v / tokenTotal < 0.01 ? 1 : 0) })}</span>
            </dd>
          </div>
        {/each}
      </dl>
    {/if}
  {/snippet}
  {#snippet table()}
    <DataTable caption={t('pages.usage.over_time')} rows={[...data].reverse()} {columns} rowKey={(r) => r.key} dense>
      {#snippet cell(row, col)}
        {#if col.key === 'bucket'}{row.label}{:else if col.key === 'total'}<strong>{tableFormat(total(row))}</strong>{:else}{tableFormat(row.values[col.key] ?? 0)}{/if}
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
  .switch {
    margin-bottom: var(--sp-3);
  }
  .warn {
    display: block;
    margin-top: var(--sp-1);
    color: var(--ink-3);
  }
  .classes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 170px), 1fr));
    gap: var(--sp-3) var(--sp-5);
    margin-top: var(--sp-5);
    padding-top: var(--sp-4);
    border-top: 1px solid var(--line);
  }
  dt {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--ink-2);
    font-weight: var(--fw-medium);
  }
  .key {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex: none;
  }
  dd {
    display: flex;
    align-items: baseline;
    gap: var(--sp-2);
    margin-top: 2px;
    padding-left: 14px;
    font-variant-numeric: tabular-nums;
  }
  dd strong {
    font-size: var(--fs-md);
    font-weight: var(--fw-semibold);
  }
  dd span {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
</style>
