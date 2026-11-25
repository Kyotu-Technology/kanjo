<script lang="ts">
  import type { Project } from '../lib/api/contract';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { fmt, t } from '../lib/i18n/index.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../lib/ui/DataTable.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';

  const projects = useQuery(() => queries.projects());
  let sortKey = $state('cost_30d');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const columns = $derived<Column<Project>[]>([
    { key: 'name', label: t('pages.projects.name'), value: (p) => p.name },
    { key: 'client', label: t('pages.projects.client'), optional: true, value: (p) => p.client ?? '' },
    { key: 'sessions', label: t('pages.projects.sessions'), align: 'end', optional: true, value: (p) => p.sessions },
    { key: 'last', label: t('pages.projects.last_seen'), optional: true, value: (p) => p.last_seen },
    { key: 'cost_30d', label: t('pages.projects.cost_30d'), align: 'end', value: (p) => p.usage_30d.cost_usd, header: badge },
    { key: 'cost_total', label: t('pages.projects.cost_total'), align: 'end', optional: true, value: (p) => p.usage_total.cost_usd },
  ]);
</script>

{#snippet badge()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}

<PageHeader title={t('pages.projects.title')} lede={t('pages.projects.lede')} />

<Card flush refreshing={projects.refreshing}>
  {#if projects.data}
    <DataTable caption={t('pages.projects.title')} rows={projects.data} {columns} rowKey={(p) => String(p.id)} bind:sortKey bind:sortDir>
      {#snippet cell(p, col)}
        {#if col.key === 'name'}<strong>{p.name}</strong>{#if p.path}<span class="sub">{p.path}</span>{/if}
        {:else if col.key === 'client'}{p.client ?? ''}
        {:else if col.key === 'sessions'}{fmt.number(p.sessions)}
        {:else if col.key === 'last'}{fmt.ago(p.last_seen)}
        {:else if col.key === 'cost_30d'}<Money usd={p.usage_30d.cost_usd} confidence="lower_bound" unpriced={p.usage_30d.unpriced_requests} size="sm" badge="none" />
        {:else}<Money usd={p.usage_total.cost_usd} confidence="lower_bound" size="sm" badge="none" />{/if}
      {/snippet}
    </DataTable>
  {:else}
    <div class="pad"><Skeleton lines={6} height="1.4em" /></div>
  {/if}
</Card>

<style>
  .sub {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .pad {
    padding: var(--sp-5);
  }
</style>
