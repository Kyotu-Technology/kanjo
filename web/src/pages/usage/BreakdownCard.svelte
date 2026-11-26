<script lang="ts">
  import type { Breakdown, BreakdownRow, Dimension } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import ShareBars from '../../lib/charts/ShareBars.svelte';
  import { modelColor } from '../../lib/charts/series';
  import { fmt, t, tk, tn } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import Select from '../../lib/ui/Select.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';
  import { rowLabel } from './labels';
  import { DIMS, ESTIMATED } from './state';

  interface Props {
    data: Breakdown | undefined;
    by: Dimension;
    /** Link that narrows the page to a row, when the dimension can be filtered. */
    linkFor: (row: BreakdownRow) => string | undefined;
    refreshing?: boolean;
  }
  let { data, by = $bindable(), linkFor, refreshing = false }: Props = $props();
  const TOP = 10;
  let sortKey = $state('cost');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const tokens = (r: BreakdownRow) => r.usage.tokens.input + r.usage.tokens.output + r.usage.tokens.cache_read + r.usage.tokens.cache_write_5m + r.usage.tokens.cache_write_1h;
  const rows = $derived(data?.rows ?? []);
  const shown = $derived(data?.by ?? by);
  const linkable = $derived(rows.some((r) => linkFor(r)));
  const columns = $derived<Column<BreakdownRow>[]>([
    { key: 'label', label: tk(`pages.usage.dims.${shown}`), value: (r) => rowLabel(shown, r) },
    { key: 'requests', label: t('chart.requests'), align: 'end', optional: true, value: (r) => r.usage.requests },
    { key: 'tokens', label: t('chart.tokens'), align: 'end', optional: true, value: tokens },
    { key: 'cost', label: t('chart.cost'), align: 'end', value: (r) => r.usage.cost_usd, header: badge },
    { key: 'share', label: t('chart.share'), align: 'end', value: (r) => r.share },
  ]);
</script>

{#snippet badge()}<ConfidenceBadge confidence={data?.confidence ?? 'lower_bound'} compact />{/snippet}

<ChartCard title={t('pages.usage.breakdown')} subtitle={t('pages.usage.breakdown_sub')} {refreshing}>
  {#snippet actions()}
    <Select label={t('pages.usage.by')} hideLabel size="sm" bind:value={by} options={DIMS.map((d) => ({ value: d, label: tk(`pages.usage.dims.${d}`) }))} />
  {/snippet}
  {#snippet note()}
    {#if data}
      <div class="note">
        <span>{linkable ? t('pages.usage.breakdown_hint') : ''}</span>
        <ConfidenceBadge confidence={data.confidence} />
      </div>
      {#if ESTIMATED.includes(shown)}
        <div class="estimate" role="note">
          <Icon name="info" size={16} />
          <div>
            <strong>{t('pages.usage.estimate_title')}</strong>
            <p>{t('pages.usage.estimate_body')}</p>
          </div>
        </div>
      {/if}
    {/if}
  {/snippet}
  {#snippet chart()}
    {#if !data}
      <Skeleton lines={6} height="1.6em" />
    {:else if !rows.length}
      <EmptyState compact icon="usage" title={t('pages.usage.breakdown_empty')} />
    {:else}
      <ShareBars
        label={t('pages.usage.breakdown')}
        shareFormat={(s) => fmt.ratio(s, s > 0 && s < 0.01 ? 1 : 0)}
        rows={rows.slice(0, TOP).map((r) => ({
          key: r.key,
          label: rowLabel(shown, r),
          value: fmt.money(r.usage.cost_usd),
          share: r.share,
          color: shown === 'model' ? modelColor(r.key) : undefined,
          href: linkFor(r),
        }))}
      />
      {#if rows.length > TOP}<p class="more">{tn('pages.usage.breakdown_more', rows.length - TOP)}</p>{/if}
    {/if}
  {/snippet}
  {#snippet table()}
    <DataTable caption={t('pages.usage.breakdown')} {rows} {columns} rowKey={(r) => r.key} bind:sortKey bind:sortDir dense>
      {#snippet cell(r, col)}
        {#if col.key === 'label'}
          {@const link = linkFor(r)}
          {#if link}<a href={link}>{rowLabel(shown, r)}</a>{:else}{rowLabel(shown, r)}{/if}
        {:else if col.key === 'requests'}{fmt.number(r.usage.requests)}
        {:else if col.key === 'tokens'}{fmt.compact(tokens(r))}
        {:else if col.key === 'cost'}<Money usd={r.usage.cost_usd} confidence={data?.confidence ?? 'lower_bound'} unpriced={r.usage.unpriced_requests} size="sm" badge="none" />
        {:else}{fmt.ratio(r.share, 1)}{/if}
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
  .estimate {
    display: flex;
    gap: var(--sp-3);
    margin-top: var(--sp-3);
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--ink-2);
  }
  .estimate :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--accent);
  }
  .estimate strong {
    display: block;
    color: var(--ink);
    font-weight: var(--fw-semibold);
  }
  .estimate p {
    margin-top: 2px;
    max-width: 72ch;
  }
  .more {
    margin-top: var(--sp-3);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
