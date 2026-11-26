<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { fmt, t, tk } from '../lib/i18n/index.svelte';
  import { describeError } from '../lib/stores/app.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import Anomalies from './insights/Anomalies.svelte';
  import CacheSection from './insights/CacheSection.svelte';
  import ContextCard from './insights/ContextCard.svelte';
  import ModelMix from './insights/ModelMix.svelte';
  import PlanFitCard from './insights/PlanFitCard.svelte';

  const insights = useQuery(() => queries.insights());
  const plan = useQuery(() => queries.plan());
  const d = $derived(insights.data);
  const p = $derived(plan.data);
  const err = $derived((insights.error && !d) || (plan.error && !p));

  const answer = $derived.by(() => {
    if (!p) return '';
    const name = tk(`plan.${p.plan.kind}`);
    const target = p.target ? tk(`plan.${p.target}`) : '';
    if (p.action === 'unknown') return p.reason === 'no_plan' ? t('pages.insights.answer.no_plan') : t('pages.insights.answer.no_history');
    if (p.action === 'upgrade') return t('pages.insights.answer.upgrade', { plan: name, target });
    if (p.action === 'downgrade')
      return p.target === 'api' || p.reason === 'api_cheaper' ? t('pages.insights.answer.api', { plan: name }) : t('pages.insights.answer.downgrade', { plan: name, target });
    if (p.reason === 'frequent_limit_hits') return t('pages.insights.answer.keep_hot', { plan: name });
    if (p.roi === null) return t('pages.insights.answer.unpriced');
    return t('pages.insights.answer.keep', { plan: name, value: fmt.money(p.api_equivalent_usd, { digits: 0 }), multiple: fmt.number(p.roi, 1) });
  });
</script>

<PageHeader title={t('pages.insights.title')} lede={t('pages.insights.lede')}>
  {#snippet meta()}
    {#if d}<span>{t('common.updated', { ago: fmt.ago(d.generated_at) })}</span>{/if}
  {/snippet}
</PageHeader>

{#if err}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(insights.error ?? plan.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => (insights.refetch(), plan.refetch())}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else}
  {#if p}
    <section class="answer" aria-labelledby="insights-answer">
      <p id="insights-answer" class="sentence">{answer}</p>
      {#if p.action !== 'unknown' && p.roi !== null}<p class="badge"><ConfidenceBadge confidence={p.confidence} /></p>{/if}
    </section>
  {:else}
    <div class="sk-answer" aria-busy="true"><Skeleton height="2.2rem" lines={2} /></div>
  {/if}

  <div class="stack">
    {#if p}<PlanFitCard plan={p} refreshing={plan.refreshing} />{:else}<Card><Skeleton lines={6} /></Card>{/if}
    {#if d}
      <div class="two">
        <ModelMix items={d.model_mix} refreshing={insights.refreshing} />
        <ContextCard items={d.context_warnings} refreshing={insights.refreshing} />
      </div>
      <CacheSection cache={d.cache} refreshing={insights.refreshing} />
      <Anomalies items={d.anomalies} refreshing={insights.refreshing} />
    {:else}
      <div class="two">
        <Card><Skeleton lines={5} /></Card>
        <Card><Skeleton lines={5} /></Card>
      </div>
      <Card><Skeleton height="200px" radius="var(--r-md)" /></Card>
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
    max-width: 32ch;
    text-wrap: balance;
  }
  .badge {
    margin-top: var(--sp-3);
  }
  .sk-answer {
    max-width: 560px;
    padding-bottom: var(--sp-6);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .two {
    display: grid;
    grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    gap: var(--sp-5);
    align-items: start;
  }
  @media (max-width: 1180px) {
    .two {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 860px) {
    .stack,
    .two {
      gap: var(--sp-4);
    }
  }
</style>
