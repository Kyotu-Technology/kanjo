<script lang="ts">
  import type { Block } from '../lib/api/contract';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import ChartCard from '../lib/charts/ChartCard.svelte';
  import LineArea from '../lib/charts/LineArea.svelte';
  import { fmt, t } from '../lib/i18n/index.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../lib/ui/DataTable.svelte';
  import LimitMeter from '../lib/ui/LimitMeter.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';

  const limits = useQuery(() => queries.limits());
  const d = $derived(limits.data);
  const series = $derived([{ key: '7d', label: t('dashboard.limits.seven_day'), color: 'var(--series-1)', points: (d?.history_7d ?? []).map((p) => ({ x: p.ts, y: p.used_pct })) }]);
  let sortKey = $state('start');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const columns = $derived<Column<Block>[]>([
    { key: 'start', label: t('pages.limits.block_start'), value: (b) => b.start },
    { key: 'peak', label: t('pages.limits.peak'), align: 'end', value: (b) => b.peak_pct ?? 0 },
    { key: 'cost', label: t('pages.limits.cost'), align: 'end', value: (b) => b.usage.cost_usd, header: badge },
  ]);
</script>

{#snippet badge()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}

<PageHeader title={t('pages.limits.title')} lede={t('pages.limits.lede')} />

{#if d}
  <div class="stack">
    <Card>
      <div class="meters">
        <LimitMeter name={t('dashboard.limits.five_hour')} window={d.current.five_hour} windowMs={5 * 3_600_000} />
        <LimitMeter name={t('dashboard.limits.seven_day')} window={d.current.seven_day} windowMs={7 * 86_400_000} />
      </div>
    </Card>
    {#if d.history_7d.length}
      <ChartCard title={t('pages.limits.history')} refreshing={limits.refreshing}>
        {#snippet chart()}
          <LineArea {series} label={t('pages.limits.history')} format={(v) => fmt.pct(v)} xFormat={(x) => fmt.dateTime(x)} xTick={(x) => fmt.date(x, 'short')} yMax={100} reference={{ y: 100, label: t('dashboard.limits.seven_day') }} />
        {/snippet}
        {#snippet table()}
          <DataTable caption={t('pages.limits.history')} rows={d.history_7d.slice(-48)} columns={[{ key: 'ts', label: t('chart.date') }, { key: 'pct', label: t('dashboard.limits.used'), align: 'end' }]} rowKey={(p) => String(p.ts)} dense>
            {#snippet cell(p, col)}{col.key === 'ts' ? fmt.dateTime(p.ts) : fmt.pct(p.used_pct)}{/snippet}
          </DataTable>
        {/snippet}
      </ChartCard>
    {/if}
    <Card title={t('pages.limits.blocks')} flush>
      <DataTable caption={t('pages.limits.blocks')} rows={d.blocks.slice(0, 12)} {columns} rowKey={(b) => String(b.start)} bind:sortKey bind:sortDir dense>
        {#snippet cell(b, col)}
          {#if col.key === 'start'}{fmt.dateTime(b.start)}{:else if col.key === 'peak'}{b.peak_pct === null ? '' : fmt.pct(b.peak_pct)}{:else}<Money usd={b.usage.cost_usd} confidence="lower_bound" size="sm" badge="none" />{/if}
        {/snippet}
      </DataTable>
    </Card>
  </div>
{:else}
  <Card><Skeleton lines={6} /></Card>
{/if}

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .meters {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: var(--sp-7);
  }
</style>
