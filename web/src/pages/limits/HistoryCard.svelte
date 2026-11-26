<script lang="ts">
  import type { LimitPoint } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import LineArea from '../../lib/charts/LineArea.svelte';
  import { fmt, i18n, t } from '../../lib/i18n/index.svelte';
  import DataTable from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Tabs from '../../lib/ui/Tabs.svelte';
  import { limitLine } from './history';

  interface Props {
    h5: LimitPoint[];
    h7: LimitPoint[];
    refreshing?: boolean;
  }
  let { h5, h7, refreshing = false }: Props = $props();
  let tab = $state<'7d' | '5h'>('7d');
  const raw = $derived(tab === '5h' ? h5 : h7);
  const points = $derived(limitLine(raw, i18n.now));
  const name = $derived(tab === '5h' ? t('dashboard.limits.five_hour') : t('dashboard.limits.seven_day'));
  const peak = $derived(raw.reduce<LimitPoint | null>((m, p) => (!m || p.used_pct > m.used_pct ? p : m), null));
  const rows = $derived([...raw].reverse());
  const columns = $derived([
    { key: 'ts', label: t('pages.limits.col_time') },
    { key: 'pct', label: t('pages.limits.col_used'), align: 'end' as const },
    { key: 'reset', label: t('pages.limits.col_resets'), align: 'end' as const, optional: true },
  ]);
</script>

<ChartCard title={t('pages.limits.history_title')} subtitle={tab === '5h' ? t('pages.limits.history_sub_5h') : t('pages.limits.history_sub_7d')} {refreshing}>
  {#snippet actions()}
    <Tabs label={t('pages.limits.history_title')} size="sm" items={[{ id: '7d', label: t('pages.limits.tab_7d') }, { id: '5h', label: t('pages.limits.tab_5h') }]} bind:value={tab} />
  {/snippet}
  {#snippet note()}
    {#if peak}{t('pages.limits.history_peak', { pct: fmt.pct(peak.used_pct), when: fmt.dateTime(peak.ts) })}{/if}
  {/snippet}
  {#snippet chart()}
    {#if points.length > 1}
      <LineArea
        series={[{ key: tab, label: name, color: 'var(--series-1)', points }]}
        label={name}
        format={(v) => fmt.pct(v)}
        xFormat={(x) => fmt.dateTime(x)}
        xTick={(x) => (tab === '5h' ? fmt.time(x) : fmt.date(x, 'short'))}
        yMax={100}
        reference={{ y: 100, label: name }}
        height={240}
      />
    {:else}
      <EmptyState compact icon="chart" title={t('pages.limits.history_empty')} />
    {/if}
  {/snippet}
  {#snippet table()}
    <DataTable caption={name} {rows} {columns} rowKey={(p) => String(p.ts)} dense empty={t('pages.limits.history_empty')}>
      {#snippet cell(p, col)}
        {#if col.key === 'ts'}{fmt.dateTime(p.ts)}{:else if col.key === 'pct'}{fmt.pct(p.used_pct)}{:else}{fmt.dateTime(p.resets_at)}{/if}
      {/snippet}
    </DataTable>
  {/snippet}
</ChartCard>
