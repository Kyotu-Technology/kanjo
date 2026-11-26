<script lang="ts">
  import type { Anomaly } from '../../lib/api/contract';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Icon from '../../lib/ui/Icon.svelte';

  interface Props {
    items: Anomaly[];
    refreshing?: boolean;
  }
  let { items, refreshing = false }: Props = $props();
  const body = (a: Anomaly) =>
    tk(`pages.insights.anomaly.body.${a.kind}`, {
      date: fmt.date(a.ts, 'long'),
      project: a.project ?? t('common.unknown'),
      value: a.kind === 'agent_loop' ? fmt.number(a.value) : fmt.money(a.value),
      baseline: a.kind === 'agent_loop' ? fmt.number(a.baseline) : fmt.money(a.baseline),
    });
</script>

<Card title={t('pages.insights.anomalies')} subtitle={t('pages.insights.cache_box.causes_sub')} {refreshing}>
  {#if items.length}
    <ul class="list">
      {#each items as a (a.kind + a.ts)}
        <li>
          <span class="icon" aria-hidden="true"><Icon name={a.kind === 'cost_spike' ? 'chart' : a.kind === 'agent_loop' ? 'refresh' : 'bolt'} size={18} /></span>
          <div class="text">
            <p class="title">{tk(`pages.insights.anomaly.kind.${a.kind}`)}<span class="when">{fmt.ago(a.ts)}</span></p>
            <p class="body">{body(a)}{#if a.kind !== 'agent_loop'} <ConfidenceBadge confidence="lower_bound" compact />{/if}</p>
            <p class="do"><strong>{t('pages.insights.next')}:</strong> {tk(`pages.insights.anomaly.do.${a.kind}`)}</p>
          </div>
          <div class="act">
            {#if a.session_id}
              <Button size="sm" href={href(`/sessions/${encodeURIComponent(a.session_id)}`)}>{t('pages.insights.anomaly.open_session')}</Button>
            {:else}
              <Button size="sm" href={href('/usage')}>{t('pages.insights.anomaly.open_usage')}</Button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState compact icon="check" title={t('pages.insights.anomaly.none')} />
  {/if}
</Card>

<style>
  .list li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: var(--sp-3) var(--sp-4);
    align-items: start;
    padding: var(--sp-4) 0;
  }
  .list li:first-child {
    padding-top: 0;
  }
  .list li + li {
    border-top: 1px solid var(--line);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--r-sm);
    background: var(--surface-3);
    color: var(--ink-2);
  }
  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--sp-1) var(--sp-3);
    font-weight: var(--fw-semibold);
  }
  .when {
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-regular);
  }
  .body {
    margin-top: 2px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .do {
    margin-top: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .do strong {
    color: var(--ink);
    font-weight: var(--fw-semibold);
  }
  @media (max-width: 640px) {
    .list li {
      grid-template-columns: auto minmax(0, 1fr);
    }
    .act {
      grid-column: 2;
    }
  }
</style>
