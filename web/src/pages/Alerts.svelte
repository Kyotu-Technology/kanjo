<script lang="ts">
  import { tick } from 'svelte';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { fmt, i18n, t, tn } from '../lib/i18n/index.svelte';
  import { describeError } from '../lib/stores/app.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import AlertList from './alerts/AlertList.svelte';
  import BudgetList from './alerts/BudgetList.svelte';

  let filter = $state<'open' | 'all'>('open');
  const alerts = useQuery(() => queries.alerts({ limit: 50, lang: i18n.lang, q: filter === 'open' ? 'open' : undefined }));
  const budgets = useQuery(() => queries.budgets());
  const projects = useQuery(() => queries.projects());
  let highlight = $state<number | null>(null);

  const a = $derived(alerts.data);
  const b = $derived(budgets.data);
  const answer = $derived(a ? (a.unacknowledged > 0 ? tn('pages.alerts.answer.open', a.unacknowledged) : t('pages.alerts.answer.clear')) : '');
  const budgetLine = $derived.by(() => {
    if (!b) return '';
    const active = b.filter((x) => x.enabled);
    if (!b.length) return t('pages.alerts.answer.budgets_none');
    const over = active.filter((x) => x.state === 'exceeded').length;
    const close = active.filter((x) => x.state === 'warning').length;
    const parts: string[] = [];
    if (over) parts.push(tn('pages.alerts.answer.budgets_exceeded', over));
    if (close) parts.push(tn('pages.alerts.answer.budgets_warning', close));
    return parts.length ? parts.join(' ') : t('pages.alerts.answer.budgets_ok');
  });

  async function showBudget(id: number) {
    highlight = id;
    await tick();
    const el = document.getElementById(`budget-${id}`);
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    el?.focus({ preventScroll: true });
    setTimeout(() => (highlight = null), 2400);
  }
</script>

<PageHeader title={t('pages.alerts.title')} lede={t('pages.alerts.lede')} />

{#if alerts.error && !a && budgets.error && !b}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(alerts.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => (alerts.refetch(), budgets.refetch())}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else}
  {#if a && b}
    <section class="answer" aria-labelledby="alerts-answer">
      <p id="alerts-answer" class="sentence">{answer}</p>
      <p class="support">{budgetLine}</p>
    </section>
  {:else}
    <div class="sk-answer" aria-busy="true"><Skeleton height="2.2rem" lines={2} /></div>
  {/if}
  <div class="stack">
    {#if alerts.error && !a}
      <Banner tone="warning" title={t('common.error_title')} body={describeError(alerts.error)}>
        {#snippet action()}<Button icon="refresh" onclick={() => alerts.refetch()}>{t('common.retry')}</Button>{/snippet}
      </Banner>
    {:else}
      <AlertList items={a?.items ?? null} total={a?.total ?? 0} unread={a?.unacknowledged ?? 0} bind:filter refreshing={alerts.refreshing} onbudget={showBudget} />
    {/if}
    {#if budgets.error && !b}
      <Banner tone="warning" title={t('common.error_title')} body={describeError(budgets.error)}>
        {#snippet action()}<Button icon="refresh" onclick={() => budgets.refetch()}>{t('common.retry')}</Button>{/snippet}
      </Banner>
    {:else}
      <BudgetList budgets={b ?? null} projects={projects.data ?? []} refreshing={budgets.refreshing} {highlight} />
    {/if}
  </div>
{/if}

<style>
  .answer {
    padding: 0 0 var(--sp-6);
  }
  .sentence {
    font-size: var(--fs-3xl);
    font-weight: var(--fw-medium);
    line-height: 1.18;
    letter-spacing: -0.025em;
    max-width: 30ch;
    text-wrap: balance;
  }
  .support {
    margin-top: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-md);
  }
  .sk-answer {
    max-width: 520px;
    padding-bottom: var(--sp-6);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  @media (max-width: 860px) {
    .stack {
      gap: var(--sp-4);
    }
  }
</style>
