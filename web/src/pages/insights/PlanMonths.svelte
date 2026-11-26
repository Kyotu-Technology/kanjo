<script lang="ts">
  import type { PlanMonth } from '../../lib/api/contract';
  import { fmt, i18n, t } from '../../lib/i18n/index.svelte';
  import { monthLabel } from './format';

  interface Props {
    months: PlanMonth[];
    subscription: number | null;
  }
  let { months, subscription }: Props = $props();
  const top = $derived(Math.max(...months.map((m) => m.api_equivalent_usd), subscription ?? 0, 1e-9) * 1.04);
  const thisMonth = $derived.by(() => {
    const d = new Date(i18n.now);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
  });
  const pos = (v: number) => (v / top) * 100;
</script>

<div class="months" role="list" aria-label={t('pages.insights.plan.months_label')}>
  {#if subscription}
    <div class="row ref-row" aria-hidden="true">
      <span></span>
      <span class="track"><span class="ref-label" style="left:{pos(subscription)}%">{t('pages.insights.plan.line', { value: fmt.money(subscription, { compact: true }) })}</span></span>
      <span></span>
    </div>
  {/if}
  {#each months as m (m.month)}
    {@const v = m.api_equivalent_usd}
    <div class="row" role="listitem">
      <span class="month">{monthLabel(m.month)}{#if m.month === thisMonth}<span class="partial">, {t('pages.insights.plan.partial')}</span>{/if}</span>
      <span class="track">
        {#if v > 0}<span class="bar" style="width:{pos(v)}%"></span>{/if}
        {#if subscription}<span class="ref" style="left:{pos(subscription)}%" aria-hidden="true"></span>{/if}
      </span>
      <span class="val">
        {#if v > 0}
          <strong>{fmt.money(v, { digits: 0 })}</strong>
          {#if subscription}<span class="times">{t('pages.insights.plan.times', { value: fmt.number(v / subscription, 1) })}</span>{/if}
        {:else}
          <span class="times">{t('pages.insights.plan.no_usage')}</span>
        {/if}
      </span>
    </div>
  {/each}
</div>

<style>
  .months {
    display: grid;
    gap: 6px;
  }
  .row {
    display: grid;
    grid-template-columns: 104px minmax(0, 1fr) 128px;
    gap: var(--sp-3);
    align-items: center;
    height: 34px;
  }
  .ref-row {
    height: 16px;
  }
  .month {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }
  .partial {
    color: var(--ink-3);
  }
  .track {
    position: relative;
    height: 100%;
  }
  .bar {
    position: absolute;
    top: 8px;
    bottom: 8px;
    left: 0;
    border-radius: 0 4px 4px 0;
    background: var(--series-1);
  }
  .ref {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 2px;
    margin-left: -1px;
    background: var(--ink-2);
    box-shadow: 0 0 0 1px var(--surface);
  }
  .ref-label {
    position: absolute;
    bottom: 0;
    transform: translateX(-50%);
    color: var(--ink-2);
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
    white-space: nowrap;
  }
  .val {
    display: flex;
    flex-direction: column;
    line-height: 1.2;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .times {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  @media (max-width: 640px) {
    .row {
      grid-template-columns: 64px minmax(0, 1fr) 92px;
      gap: var(--sp-2);
    }
    .partial {
      display: none;
    }
    .val {
      white-space: normal;
    }
  }
</style>
