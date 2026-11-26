<script lang="ts">
  import type { LimitWindow } from '../../lib/api/contract';
  import { dayKey, startOfLocalDay } from '../../lib/i18n/format';
  import { fmt, i18n, t, tk, tn } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';

  interface Props {
    week: LimitWindow;
    refreshing?: boolean;
  }
  let { week, refreshing = false }: Props = $props();
  const DAY = 86_400_000;
  const used = $derived(Math.max(0, Math.min(100, week.used_pct)));
  const left = $derived(100 - used);
  const span = $derived(Math.max(1, week.resets_at - i18n.now));
  const perDay = $derived(week.daily_budget_pct ?? (left / span) * DAY);
  const days = $derived.by(() => {
    const out: { key: string; label: string; share: number }[] = [];
    let from = i18n.now;
    while (from < week.resets_at && out.length < 8) {
      const to = Math.min(week.resets_at, startOfLocalDay(from) + DAY);
      out.push({ key: String(from), label: tk(`chart.weekday_short.${dayKey(from)}`), share: (to - from) / span });
      from = to;
    }
    return out;
  });
</script>

<Card title={t('pages.limits.budget.title')} {refreshing}>
  {#snippet actions()}<ConfidenceBadge confidence="estimate" />{/snippet}
  {#if left > 0.5}
    <p class="value">{t('pages.limits.budget.value', { pct: fmt.pct(perDay, perDay < 10 ? 1 : 0) })}</p>
    <p class="body">{t('pages.limits.budget.body', { left: fmt.pct(left), when: fmt.when(week.resets_at), pct: fmt.pct(perDay, perDay < 10 ? 1 : 0) })}</p>
    <div class="bar" aria-hidden="true">
      <span class="gone" style="flex:{used}"></span>
      {#each days as d (d.key)}<span class="day" style="flex:{d.share * left}"></span>{/each}
    </div>
    <div class="labels" aria-hidden="true">
      <span style="flex:{used}" class="gone-l">{t('pages.limits.budget.spent')} {fmt.pct(used)}</span>
      {#each days as d (d.key)}<span class="day-l" style="flex:{d.share * left}">{d.share * left > 4 ? d.label : ''}</span>{/each}
    </div>
    <p class="meta">{tn('pages.limits.budget.days', Math.max(1, Math.round(span / DAY)))}</p>
  {:else}
    <p class="body">{t('pages.limits.budget.none', { when: fmt.when(week.resets_at) })}</p>
  {/if}
</Card>

<style>
  .value {
    font-size: var(--fs-2xl);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.02em;
    line-height: 1.1;
  }
  .body {
    margin-top: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 52ch;
  }
  .bar {
    display: flex;
    gap: 2px;
    height: 14px;
    margin-top: var(--sp-5);
  }
  .bar span {
    min-width: 2px;
    border-radius: 3px;
  }
  .gone {
    background: var(--meter-fill);
  }
  .day {
    background: var(--meter-track);
  }
  .labels {
    display: flex;
    gap: 2px;
    margin-top: var(--sp-2);
    font-size: var(--fs-xs);
    color: var(--ink-3);
    white-space: nowrap;
  }
  .labels span {
    min-width: 2px;
    overflow: hidden;
  }
  .gone-l {
    color: var(--ink-2);
  }
  .day-l {
    text-align: center;
  }
  .meta {
    margin-top: var(--sp-3);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
