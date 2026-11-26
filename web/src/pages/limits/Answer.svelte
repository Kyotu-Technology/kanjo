<script lang="ts">
  import type { Limits } from '../../lib/api/contract';
  import { pickForecast } from '../../lib/forecast';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';

  interface Props {
    limits: Limits;
  }
  let { limits }: Props = $props();
  const f = $derived(pickForecast(limits));
  const MARK = '{limit}';
  const limitName = $derived(
    f.kind === 'exhaust' || f.kind === 'lasts' ? t(f.window === 'five_hour' ? 'dashboard.forecast.limit_5h' : 'dashboard.forecast.limit_7d') : '',
  );
  const sentence = $derived(
    f.kind === 'exhaust'
      ? t('dashboard.forecast.exhaust', { limit: MARK, when: fmt.when(f.at) })
      : f.kind === 'lasts'
        ? t('dashboard.forecast.lasts', { limit: MARK, when: fmt.when(f.resetsAt) })
        : '',
  );
  const support = $derived.by(() => {
    const parts: string[] = [];
    if (f.kind === 'exhaust') parts.push(t('dashboard.forecast.reset_note', { when: fmt.when(f.resetsAt) }));
    const rate = limits.seven_day?.rate_pct_per_hour;
    if (rate) parts.push(t('pages.limits.budget.pace', { rate: fmt.pct(rate, rate < 10 ? 1 : 0) }));
    return parts.join(' ');
  });
</script>

{#if sentence}
  <section class="answer" aria-labelledby="limits-answer">
    <p id="limits-answer" class="sentence" class:urgent={f.kind === 'exhaust'}>
      {#each sentence.split(MARK) as part, i (i)}{#if i > 0}<span class="nowrap">{limitName}</span>{/if}{part}{/each}
    </p>
    {#if support}<p class="support">{support}</p>{/if}
    {#if f.kind === 'exhaust'}<p class="badge"><ConfidenceBadge confidence="estimate" /></p>{/if}
  </section>
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
  .nowrap {
    white-space: nowrap;
  }
  .support {
    margin-top: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-md);
    max-width: 60ch;
  }
  .badge {
    margin-top: var(--sp-3);
  }
</style>
