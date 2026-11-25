<script lang="ts">
  import type { SessionSummary } from '../lib/api/contract';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { modelLabel } from '../lib/charts/series';
  import { fmt, t, tn } from '../lib/i18n/index.svelte';
  import { href } from '../lib/router';
  import { navigate } from '../lib/stores/router.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../lib/ui/DataTable.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';

  let q = $state('');
  let debounced = $state('');
  $effect(() => {
    const v = q;
    const id = setTimeout(() => (debounced = v), 250);
    return () => clearTimeout(id);
  });
  const list = useQuery(() => queries.sessions({ q: debounced, limit: 100, sort: 'cost' }));
  let sortKey = $state('cost');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const columns = $derived<Column<SessionSummary>[]>([
    { key: 'session', label: t('dashboard.sessions.session'), value: (s) => s.title ?? s.project },
    { key: 'model', label: t('dashboard.sessions.model'), optional: true, value: (s) => s.main_model ?? '' },
    { key: 'last', label: t('pages.sessions.last'), optional: true, value: (s) => s.last_at },
    { key: 'cache', label: t('pages.sessions.cache'), align: 'end', optional: true, value: (s) => s.cache_hit_ratio ?? 0 },
    { key: 'cost', label: t('dashboard.sessions.cost'), align: 'end', value: (s) => s.usage.cost_usd, header: badge },
  ]);
</script>

{#snippet badge()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}

<PageHeader title={t('pages.sessions.title')} lede={t('pages.sessions.lede')}>
  {#snippet filters()}
    <label class="search">
      <span class="sr-only">{t('pages.sessions.search')}</span>
      <input type="search" bind:value={q} placeholder={t('pages.sessions.search')} />
    </label>
    {#if list.data}<span class="count">{tn('pages.sessions.count', list.data.total)}</span>{/if}
  {/snippet}
</PageHeader>

<Card flush refreshing={list.refreshing}>
  {#if list.data}
    <DataTable
      caption={t('pages.sessions.title')}
      rows={list.data.items}
      {columns}
      rowKey={(s) => s.id}
      bind:sortKey
      bind:sortDir
      onrowclick={(s) => navigate(href(`/sessions/${encodeURIComponent(s.id)}`))}
    >
      {#snippet cell(s, col)}
        {#if col.key === 'session'}
          <a class="title" href={href(`/sessions/${encodeURIComponent(s.id)}`)} onclick={(e) => e.stopPropagation()}>{s.title ?? t('dashboard.sessions.untitled')}</a>
          <span class="sub">{s.project}{#if s.git_branch}, {s.git_branch}{/if}{#if s.is_active}, {t('pages.sessions.active')}{/if}</span>
        {:else if col.key === 'model'}{s.main_model ? modelLabel(s.main_model) : ''}
        {:else if col.key === 'last'}{fmt.dateTime(s.last_at)}
        {:else if col.key === 'cache'}{s.cache_hit_ratio === null ? '' : fmt.ratio(s.cache_hit_ratio)}
        {:else}<Money usd={s.usage.cost_usd} confidence="lower_bound" unpriced={s.usage.unpriced_requests} size="sm" badge="none" />{/if}
      {/snippet}
    </DataTable>
  {:else}
    <div class="pad"><Skeleton lines={8} height="1.4em" /></div>
  {/if}
</Card>

<style>
  .search input {
    min-height: 36px;
    width: min(360px, 80vw);
    padding: 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    font-size: var(--fs-sm);
  }
  .count {
    color: var(--ink-3);
    font-size: var(--fs-sm);
    align-self: center;
  }
  .title {
    display: block;
    color: var(--ink);
    font-weight: var(--fw-semibold);
    text-decoration: none;
  }
  .sub {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .pad {
    padding: var(--sp-5);
  }
</style>
