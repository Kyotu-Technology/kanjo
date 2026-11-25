<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { fmt, i18n, t } from '../lib/i18n/index.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Severity from '../lib/ui/Severity.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';

  const alerts = useQuery(() => queries.alerts({ limit: 50, lang: i18n.lang }));
  const budgets = useQuery(() => queries.budgets());
</script>

<PageHeader title={t('pages.alerts.title')} lede={t('pages.alerts.lede')} />

<div class="grid">
  <Card title={t('pages.alerts.alerts')} refreshing={alerts.refreshing}>
    {#if alerts.data}
      {#if alerts.data.items.length}
        <ul class="list">
          {#each alerts.data.items as a (a.id)}
            <li class:read={a.acknowledged_at !== null}>
              <Severity severity={a.severity} />
              <div>
                <p class="title">{a.title}</p>
                <p class="msg">{a.message}</p>
                <p class="meta">{fmt.dateTime(a.ts)}</p>
              </div>
            </li>
          {/each}
        </ul>
      {:else}
        <EmptyState compact icon="check" title={t('dashboard.alerts.empty')} />
      {/if}
    {:else}
      <Skeleton lines={5} />
    {/if}
  </Card>
  <Card title={t('pages.alerts.budgets')} refreshing={budgets.refreshing}>
    {#snippet actions()}<ConfidenceBadge confidence="lower_bound" />{/snippet}
    {#if budgets.data}
      {#if budgets.data.length}
        <ul class="budgets">
          {#each budgets.data as b (b.id)}
            <li>
              <div class="bhead">
                <p class="title">{b.name}</p>
                <span class="state {b.state}">{t(`pages.alerts.state.${b.state}`)}</span>
              </div>
              <div class="track"><span class={b.state} style="width:{Math.min(100, b.spent_pct)}%"></span></div>
              <p class="meta">
                {t('pages.alerts.spent_of', { spent: fmt.money(b.spent_usd), amount: fmt.money(b.amount_usd) })}. {t('pages.alerts.projected', { value: fmt.money(b.projected_usd) })}
                <ConfidenceBadge confidence="estimate" compact />
              </p>
            </li>
          {/each}
        </ul>
      {:else}
        <EmptyState compact title={t('pages.alerts.no_budgets')} />
      {/if}
    {:else}
      <Skeleton lines={4} />
    {/if}
  </Card>
</div>
<p class="preview">{t('pages.preview')}</p>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--sp-5);
    align-items: start;
  }
  .list li,
  .budgets li {
    display: flex;
    gap: var(--sp-3);
    padding: var(--sp-3) 0;
  }
  .budgets li {
    flex-direction: column;
    gap: var(--sp-2);
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  .read {
    opacity: 0.7;
  }
  .title {
    font-weight: var(--fw-semibold);
  }
  .msg {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .meta {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .bhead {
    display: flex;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  .state {
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
    padding: 2px 8px;
    border-radius: var(--r-pill);
    background: var(--surface-3);
    color: var(--ink-2);
  }
  .state.warning {
    background: var(--warning-soft);
    color: var(--warning-ink);
  }
  .state.exceeded {
    background: var(--critical-soft);
    color: var(--critical-ink);
  }
  .track {
    height: 8px;
    border-radius: var(--r-pill);
    background: var(--meter-track);
  }
  .track span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--meter-fill);
  }
  .track span.warning {
    background: var(--warning);
  }
  .track span.exceeded {
    background: var(--critical);
  }
  .preview {
    margin-top: var(--sp-5);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
