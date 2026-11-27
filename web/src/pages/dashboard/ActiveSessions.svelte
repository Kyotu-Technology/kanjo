<script lang="ts">
  import type { Confidence, SessionSummary } from '../../lib/api/contract';
  import { modelColor, modelLabel, modelTitle } from '../../lib/charts/series';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import { untitled } from '../../lib/session-title';
  import { navigate } from '../../lib/stores/router.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Money from '../../lib/ui/Money.svelte';

  interface Props {
    sessions: SessionSummary[];
    confidence: Confidence;
    refreshing?: boolean;
  }
  let { sessions, confidence, refreshing = false }: Props = $props();
  let sortKey = $state('cost');
  let sortDir = $state<'asc' | 'desc'>('desc');

  const columns = $derived<Column<SessionSummary>[]>([
    { key: 'session', label: t('dashboard.sessions.session'), value: (s) => s.title ?? s.project },
    { key: 'model', label: t('dashboard.sessions.model'), optional: true, value: (s) => s.main_model ?? '' },
    { key: 'started', label: t('dashboard.sessions.started'), optional: true, value: (s) => s.started_at },
    { key: 'context', label: t('dashboard.sessions.context'), optional: true, value: (s) => s.context_pct ?? 0, width: '160px' },
    { key: 'cost', label: t('dashboard.sessions.cost'), align: 'end', value: (s) => s.usage.cost_usd, header: costHeader },
  ]);
</script>

{#snippet costHeader()}<ConfidenceBadge {confidence} compact />{/snippet}

<Card title={t('dashboard.sessions.title')} flush {refreshing}>
  {#snippet actions()}
    <a class="all" href={href('/sessions')}>{t('dashboard.sessions.all')}</a>
  {/snippet}
  {#if sessions.length}
    <DataTable
      caption={t('dashboard.sessions.title')}
      rows={sessions}
      {columns}
      rowKey={(s) => s.id}
      bind:sortKey
      bind:sortDir
      onrowclick={(s) => navigate(href(`/sessions/${encodeURIComponent(s.id)}`))}
    >
      {#snippet cell(s, col)}
        {#if col.key === 'session'}
          <a class="title" href={href(`/sessions/${encodeURIComponent(s.id)}`)} onclick={(e) => e.stopPropagation()}>{s.title ?? untitled()}</a>
          <span class="sub">{s.project}{#if s.git_branch}, {s.git_branch}{/if}</span>
        {:else if col.key === 'model'}
          {#if s.main_model}<span class="model" title={modelTitle(s.main_model)}><span class="key" style="background:{modelColor(s.main_model)}"></span>{modelLabel(s.main_model)}</span>{/if}
        {:else if col.key === 'started'}
          {fmt.time(s.started_at)}<span class="sub">{fmt.ago(s.last_at)}</span>
        {:else if col.key === 'context'}
          {#if s.context_pct !== null}
            <span class="ctx" class:high={s.context_pct >= 85}>
              <span class="ctx-bar"><span style="width:{s.context_pct}%"></span></span>
              {fmt.pct(s.context_pct)}
            </span>
          {/if}
        {:else}
          <Money usd={s.usage.cost_usd} {confidence} unpriced={s.usage.unpriced_requests} size="sm" badge="none" />
        {/if}
      {/snippet}
    </DataTable>
  {:else}
    <div class="pad"><EmptyState compact icon="sessions" title={t('dashboard.sessions.empty')} /></div>
  {/if}
</Card>

<style>
  .all {
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    white-space: nowrap;
  }
  .title {
    display: block;
    color: var(--ink);
    font-weight: var(--fw-semibold);
    text-decoration: none;
  }
  .title:hover {
    text-decoration: underline;
  }
  .sub {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .model {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  .key {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .ctx {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    width: 100%;
  }
  .ctx-bar {
    flex: 1;
    height: 6px;
    border-radius: var(--r-pill);
    background: var(--meter-track);
    min-width: 50px;
  }
  .ctx-bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--meter-fill);
  }
  .ctx.high .ctx-bar {
    background: var(--warning-soft);
  }
  .ctx.high .ctx-bar span {
    background: var(--warning);
  }
  .pad {
    padding: 0 var(--sp-5) var(--sp-4);
  }
</style>
