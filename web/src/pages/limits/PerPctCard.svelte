<script lang="ts">
  import type { UsdPerPctPoint } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import StackedBars, { type BarDatum } from '../../lib/charts/StackedBars.svelte';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Money from '../../lib/ui/Money.svelte';

  interface Props {
    weeks: UsdPerPctPoint[];
    refreshing?: boolean;
  }
  let { weeks, refreshing = false }: Props = $props();
  const read = $derived(weeks.filter((w) => w.usd_per_pct !== null));
  const last = $derived(weeks.at(-1));
  const current = $derived(last && last.usd_per_pct !== null ? last : null);
  const latest = $derived(read.at(-1) ?? null);
  const low = $derived(Math.min(...read.map((w) => w.usd_per_pct ?? Infinity)));
  const high = $derived(Math.max(...read.map((w) => w.usd_per_pct ?? -Infinity)));
  const series = $derived([{ key: 'v', label: t('pages.limits.per_pct_box.series'), color: 'var(--series-1)' }]);
  const data = $derived<BarDatum[]>(
    weeks.map((w) => ({ key: String(w.week_start), label: fmt.date(w.week_start, 'medium'), tick: fmt.date(w.week_start, 'short'), values: { v: w.usd_per_pct ?? 0 } })),
  );
  const rows = $derived([...weeks].reverse());
  let sortKey = $state('week');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const columns = $derived<Column<UsdPerPctPoint>[]>([
    { key: 'week', label: t('pages.limits.per_pct_box.col_week'), value: (w) => w.week_start },
    { key: 'peak', label: t('pages.limits.per_pct_box.col_peak'), align: 'end', value: (w) => w.pct, optional: true },
    { key: 'cost', label: t('pages.limits.per_pct_box.col_cost'), align: 'end', value: (w) => w.usd, header: lower },
    { key: 'per', label: t('pages.limits.per_pct_box.col_per_pct'), align: 'end', value: (w) => w.usd_per_pct ?? -1, header: est },
  ]);
</script>

{#snippet lower()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}
{#snippet est()}<ConfidenceBadge confidence="estimate" compact />{/snippet}

<ChartCard title={t('pages.limits.per_pct_box.title')} subtitle={t('pages.limits.per_pct_box.subtitle')} {refreshing}>
  {#snippet note()}
    {#if latest}
      <div class="story">
        <p class="lead">
          {current
            ? t('pages.limits.per_pct_box.sentence', { value: fmt.money(current.usd_per_pct ?? 0) })
            : t('pages.limits.per_pct_box.last', { value: fmt.money(latest.usd_per_pct ?? 0) })}
          <ConfidenceBadge confidence="estimate" compact />
        </p>
        <p>
          {#if read.length > 1}{fmt.money(low) === fmt.money(high) ? t('pages.limits.per_pct_box.range_flat', { value: fmt.money(low) }) : t('pages.limits.per_pct_box.range', { low: fmt.money(low), high: fmt.money(high) })}{/if}
          {t('pages.limits.per_pct_box.explain')}
          {#if read.length < weeks.length}{t('pages.limits.per_pct_box.missing')}{/if}
        </p>
      </div>
    {/if}
  {/snippet}
  {#snippet chart()}
    {#if read.length}
      <StackedBars
        {data}
        {series}
        label={t('pages.limits.per_pct_box.title')}
        format={(v) => (v > 0 ? fmt.money(v) : t('pages.limits.timeline.no_peak'))}
        axisFormat={(v) => fmt.money(v, { digits: v < 10 ? 2 : 0 })}
        height={200}
      />
    {:else}
      <EmptyState compact icon="chart" title={t('pages.limits.per_pct_box.empty')} />
    {/if}
  {/snippet}
  {#snippet table()}
    <DataTable caption={t('pages.limits.per_pct_box.title')} {rows} {columns} rowKey={(w) => String(w.week_start)} bind:sortKey bind:sortDir dense>
      {#snippet cell(w, col)}
        {#if col.key === 'week'}{fmt.date(w.week_start, 'medium')}
        {:else if col.key === 'peak'}{w.usd_per_pct === null ? t('pages.limits.timeline.no_peak') : fmt.pct(w.pct)}
        {:else if col.key === 'cost'}<Money usd={w.usd} confidence="lower_bound" size="sm" badge="none" />
        {:else if w.usd_per_pct === null}<span class="quiet">{t('pages.limits.timeline.no_peak')}</span>
        {:else}<Money usd={w.usd_per_pct} confidence="estimate" size="sm" badge="none" />{/if}
      {/snippet}
    </DataTable>
  {/snippet}
</ChartCard>

<style>
  .story {
    display: grid;
    gap: var(--sp-1);
    max-width: 72ch;
  }
  .lead {
    color: var(--ink);
    font-size: var(--fs-md);
    font-weight: var(--fw-medium);
  }
  .lead :global(.badge) {
    vertical-align: middle;
  }
  .quiet {
    color: var(--ink-3);
  }
</style>
