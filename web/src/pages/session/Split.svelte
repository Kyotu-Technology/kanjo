<script lang="ts">
  import type { BreakdownRow, Confidence } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import ShareBars from '../../lib/charts/ShareBars.svelte';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import Money from '../../lib/ui/Money.svelte';

  interface Props {
    title: string;
    rows: BreakdownRow[];
    confidence: Confidence;
    label: (r: BreakdownRow) => string;
    tooltip?: (r: BreakdownRow) => string | undefined;
    color?: (r: BreakdownRow) => string | undefined;
    help?: string;
    nameColumn: string;
    max?: number;
  }
  let { title, rows, confidence, label, tooltip, color, help, nameColumn, max = 8 }: Props = $props();
  let sortKey = $state('cost');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const columns = $derived<Column<BreakdownRow>[]>([
    { key: 'name', label: nameColumn, value: (r) => label(r) },
    { key: 'requests', label: t('chart.requests'), align: 'end', value: (r) => r.usage.requests },
    { key: 'cost', label: t('chart.cost'), align: 'end', value: (r) => r.usage.cost_usd, header: badge },
    { key: 'share', label: t('chart.share'), align: 'end', value: (r) => r.share },
  ]);
</script>

{#snippet badge()}<ConfidenceBadge {confidence} compact />{/snippet}

<ChartCard {title}>
  {#snippet actions()}{#if help}<InlineHelp text={help} label={title} />{/if}{/snippet}
  {#snippet note()}<ConfidenceBadge {confidence} />{/snippet}
  {#snippet chart()}
    {#if rows.length}
      <ShareBars
        label={title}
        shareFormat={(s) => fmt.ratio(s, s > 0 && s < 0.01 ? 1 : 0)}
        rows={rows.slice(0, max).map((r) => ({ key: r.key, label: label(r), title: tooltip?.(r), value: fmt.money(r.usage.cost_usd), share: r.share, color: color?.(r) }))}
      />
    {:else}
      <EmptyState compact title={t('chart.no_data')} />
    {/if}
  {/snippet}
  {#snippet table()}
    <DataTable caption={title} {rows} {columns} rowKey={(r) => r.key} bind:sortKey bind:sortDir dense>
      {#snippet cell(r, col)}
        {#if col.key === 'name'}<span title={tooltip?.(r)}>{label(r)}</span>{:else if col.key === 'requests'}{fmt.number(r.usage.requests)}{:else if col.key === 'cost'}<Money
            usd={r.usage.cost_usd}
            {confidence}
            unpriced={r.usage.unpriced_requests}
            size="sm"
            badge="none"
          />{:else}{fmt.ratio(r.share, 1)}{/if}
      {/snippet}
    </DataTable>
  {/snippet}
</ChartCard>
