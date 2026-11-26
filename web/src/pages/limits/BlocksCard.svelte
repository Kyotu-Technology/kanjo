<script lang="ts">
  import type { Block } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import { fmt, i18n, t, tn } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import { modelLabel, modelTitle } from '../../lib/charts/series';
  import BlockTimeline from './BlockTimeline.svelte';
  import { topRows } from './blocks';

  interface Props {
    blocks: Block[];
    refreshing?: boolean;
  }
  let { blocks, refreshing = false }: Props = $props();
  const week = $derived.by(() => {
    const d = new Date(i18n.now);
    d.setHours(0, 0, 0, 0);
    d.setDate(d.getDate() - 6);
    return blocks.filter((b) => b.end > d.getTime());
  });
  const anchored = $derived(week.filter((b) => b.anchored).length);
  let sortKey = $state('start');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const columns = $derived<Column<Block>[]>([
    { key: 'start', label: t('pages.limits.block_start'), value: (b) => b.start },
    { key: 'end', label: t('pages.limits.timeline.col_end'), value: (b) => b.end, optional: true },
    { key: 'kind', label: t('pages.limits.timeline.col_kind'), value: (b) => (b.anchored ? 1 : 0) },
    { key: 'peak', label: t('pages.limits.peak'), align: 'end', value: (b) => b.peak_pct ?? -1 },
    { key: 'requests', label: t('pages.limits.timeline.col_requests'), align: 'end', value: (b) => b.usage.requests, optional: true },
    { key: 'projects', label: t('pages.limits.timeline.col_projects'), optional: true },
    { key: 'models', label: t('pages.limits.timeline.col_models'), optional: true },
    { key: 'cost', label: t('pages.limits.cost'), align: 'end', value: (b) => b.usage.cost_usd, header: badge },
  ]);
</script>

{#snippet badge()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}

<ChartCard title={t('pages.limits.timeline.title')} subtitle={t('pages.limits.timeline.subtitle')} {refreshing}>
  {#snippet actions()}
    <InlineHelp text={`${t('pages.limits.timeline.anchored')}: ${t('pages.limits.timeline.anchored_help')} ${t('pages.limits.timeline.inferred')}: ${t('pages.limits.timeline.inferred_help')}`} />
  {/snippet}
  {#snippet note()}
    {#if week.length}<ConfidenceBadge confidence="lower_bound" compact /> {anchored ? tn('pages.limits.timeline.summary', week.length, { anchored: fmt.number(anchored) }) : tn('pages.limits.timeline.summary_none', week.length)}{/if}
  {/snippet}
  {#snippet chart()}
    {#if week.length}
      <BlockTimeline blocks={week} />
    {:else}
      <EmptyState compact icon="clock" title={t('pages.limits.timeline.empty')} />
    {/if}
  {/snippet}
  {#snippet table()}
    <DataTable caption={t('pages.limits.blocks')} rows={blocks} {columns} rowKey={(b) => String(b.start)} bind:sortKey bind:sortDir dense empty={t('pages.limits.timeline.empty')}>
      {#snippet cell(b, col)}
        {#if col.key === 'start'}
          {fmt.dateTime(b.start)}{#if b.is_active}<span class="live">{t('pages.limits.timeline.running')}</span>{/if}
        {:else if col.key === 'end'}
          {fmt.time(b.end)}
        {:else if col.key === 'kind'}
          <span class="kind"><span class="sw" class:inferred={!b.anchored} aria-hidden="true"></span>{b.anchored ? t('pages.limits.timeline.kind_anchored') : t('pages.limits.timeline.kind_inferred')}</span>
        {:else if col.key === 'peak'}
          {#if b.peak_pct === null}<span class="quiet">{t('pages.limits.timeline.no_peak')}</span>{:else}{fmt.pct(b.peak_pct)}{/if}
        {:else if col.key === 'requests'}
          {fmt.number(b.usage.requests)}
        {:else if col.key === 'projects' || col.key === 'models'}
          {@const list = topRows(col.key === 'projects' ? b.by_project : b.by_model)}
          <span class="split">
            {#each list.top as r (r.key)}
              <span title={col.key === 'models' ? modelTitle(r.key) : undefined}>{col.key === 'models' ? modelLabel(r.key) : r.label} <span class="amt">{fmt.money(r.usage.cost_usd)}</span></span>
            {/each}
            {#if list.more}<span class="quiet">{t('pages.limits.timeline.more', { count: fmt.number(list.more) })}</span>{/if}
          </span>
        {:else}
          <Money usd={b.usage.cost_usd} confidence="lower_bound" size="sm" badge="none" />
        {/if}
      {/snippet}
    </DataTable>
  {/snippet}
</ChartCard>

<style>
  .kind {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    white-space: nowrap;
  }
  .sw {
    width: 16px;
    height: 10px;
    border-radius: 2px;
    background: var(--seq-5);
  }
  .sw.inferred {
    background-image: repeating-linear-gradient(135deg, transparent 0 3px, var(--surface) 3px 5px);
    outline: 1px dashed var(--ink-3);
    outline-offset: -1px;
  }
  .live {
    margin-left: var(--sp-2);
    padding: 1px 6px;
    border-radius: var(--r-pill);
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
  }
  .quiet {
    color: var(--ink-3);
  }
  .split {
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: var(--fs-xs);
    white-space: nowrap;
  }
  .amt {
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
</style>
