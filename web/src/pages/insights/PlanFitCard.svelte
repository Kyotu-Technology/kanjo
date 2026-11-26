<script lang="ts">
  import type { PlanFit } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import { fmt, t, tk, tn } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable from '../../lib/ui/DataTable.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import { monthLabel } from './format';
  import NextStep from './NextStep.svelte';
  import { nextStepKey } from './plan';
  import PlanMonths from './PlanMonths.svelte';

  interface Props {
    plan: PlanFit;
    refreshing?: boolean;
  }
  let { plan: p, refreshing = false }: Props = $props();
  const KNOWN = ['within_plan', 'frequent_limit_hits', 'low_utilisation', 'api_cheaper', 'no_plan', 'no_history'];
  const reason = $derived(
    p.reason === 'frequent_limit_hits' && p.action === 'keep'
      ? t('pages.insights.plan.reason.frequent_limit_hits_top')
      : KNOWN.includes(p.reason)
        ? tk(`pages.insights.plan.reason.${p.reason}`)
        : t('pages.insights.plan.reason.other', { reason: p.reason.replace(/_/g, ' ') }),
  );
  const hasMonths = $derived(p.months.some((m) => m.api_equivalent_usd > 0));
  const columns = $derived([
    { key: 'month', label: t('pages.insights.plan.month') },
    { key: 'value', label: t('pages.insights.plan.value'), align: 'end' as const },
    { key: 'price', label: t('pages.insights.plan.price'), align: 'end' as const },
  ]);
</script>

{#snippet links()}
  {#if p.action === 'unknown' && p.reason === 'no_plan'}
    <Button size="sm" href={href('/settings', { section: 'plan' })}>{t('pages.insights.plan.settings')}</Button>
  {:else if p.action === 'upgrade' || nextStepKey(p) === 'keep_hot'}
    <Button size="sm" href={href('/limits')}>{t('pages.insights.plan.limits')}</Button>
  {/if}
{/snippet}

<div class="plan">
  <Card title={t('pages.insights.plan.title')} subtitle={t('pages.insights.plan.subtitle')} {refreshing}>
    <div class="verdict">
      <p class="action {p.action}">{tk(`pages.insights.plan.action.${p.action}`)}</p>
      <p class="reason">{reason}</p>
      {#if p.target}<p class="target">{t('pages.insights.plan.target', { plan: tk(`plan.${p.target}`) })}</p>{/if}
    </div>
    <dl class="facts">
      {#if p.api_equivalent_usd > 0}
      <div>
        <dt>{t('pages.insights.plan.value')}<InlineHelp text={t('pages.insights.plan.value_help')} /></dt>
        <dd><Money usd={p.api_equivalent_usd} confidence={p.confidence} size="lg" /></dd>
      </div>
      {/if}
      <div>
        <dt>{t('pages.insights.plan.price')}<InlineHelp text={t('pages.insights.plan.price_help')} /></dt>
        <dd>{#if p.subscription_usd !== null}<Money usd={p.subscription_usd} confidence={p.confidence} size="lg" badge="none" />{:else}<span class="quiet">{t('common.unknown')}</span>{/if}</dd>
      </div>
      {#if p.roi !== null && p.api_equivalent_usd > 0}
        <div>
          <dt>{t('pages.insights.plan.multiple', { unit: fmt.money(1) })}</dt>
          <dd class="roi">{t('pages.insights.plan.multiple_value', { value: fmt.money(p.roi) })}</dd>
        </div>
      {/if}
      {#if p.max_five_hour_pct !== null || p.max_seven_day_pct !== null}
        <div>
          <dt>{t('pages.insights.plan.peaks')}</dt>
          <dd class="peaks">
            {[
              p.max_five_hour_pct !== null ? t('pages.insights.plan.peak_5h', { pct: fmt.pct(p.max_five_hour_pct) }) : null,
              p.max_seven_day_pct !== null ? t('pages.insights.plan.peak_7d', { pct: fmt.pct(p.max_seven_day_pct) }) : null,
            ]
              .filter(Boolean)
              .join(', ')}
            {#if p.max_five_hour_pct !== null}<span class="near">{tn('pages.insights.plan.near', p.blocks_near_limit)}</span>{/if}
          </dd>
        </div>
      {/if}
    </dl>
    <NextStep text={tk(`pages.insights.plan.do.${nextStepKey(p)}`)} children={links} />
  </Card>
  {#if hasMonths}
    <ChartCard title={t('pages.insights.plan.months_title')} subtitle={t('pages.insights.plan.months_subtitle')} {refreshing}>
      {#snippet note()}<ConfidenceBadge confidence={p.confidence} />{/snippet}
      {#snippet chart()}
        <PlanMonths months={p.months} subscription={p.subscription_usd} />
      {/snippet}
      {#snippet table()}
        <DataTable caption={t('pages.insights.plan.months_title')} rows={[...p.months].reverse()} {columns} rowKey={(m) => m.month} dense>
          {#snippet cell(m, col)}
            {#if col.key === 'month'}{monthLabel(m.month)}
            {:else if col.key === 'value'}<Money usd={m.api_equivalent_usd} confidence={p.confidence} size="sm" badge="seal" />
            {:else if m.subscription_usd !== null}{fmt.money(m.subscription_usd)}{:else}{t('common.unknown')}{/if}
          {/snippet}
        </DataTable>
      {/snippet}
    </ChartCard>
  {/if}
</div>

<style>
  .plan {
    display: grid;
    grid-template-columns: minmax(0, 6fr) minmax(0, 5fr);
    gap: var(--sp-5);
  }
  .plan > :global(.card) {
    min-width: 0;
  }
  .plan > :global(.card:only-child) {
    grid-column: 1 / -1;
  }
  .verdict {
    margin-bottom: var(--sp-5);
  }
  .action {
    display: inline-block;
    padding: 2px 10px;
    border-radius: var(--r-pill);
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .action.unknown {
    background: var(--surface-3);
    color: var(--ink-2);
  }
  .reason {
    margin-top: var(--sp-2);
    color: var(--ink);
    font-size: var(--fs-md);
    max-width: 62ch;
  }
  .target {
    margin-top: var(--sp-1);
    color: var(--ink-2);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-5) var(--sp-6);
  }
  dt {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    margin-bottom: var(--sp-1);
  }
  dd {
    color: var(--ink);
  }
  .roi {
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.015em;
  }
  .peaks {
    font-size: var(--fs-md);
    font-weight: var(--fw-medium);
  }
  .near {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
    font-weight: var(--fw-regular);
  }
  .quiet {
    color: var(--ink-3);
  }
  @media (max-width: 1180px) {
    .plan {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 480px) {
    .facts {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
