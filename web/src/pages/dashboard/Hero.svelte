<script lang="ts">
  import type { Limits } from '../../lib/api/contract';
  import { pickForecast } from '../../lib/forecast';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import LimitMeter from '../../lib/ui/LimitMeter.svelte';

  interface Props {
    limits: Limits;
  }
  let { limits }: Props = $props();
  const f = $derived(pickForecast(limits));
  const limitName = (w: 'five_hour' | 'seven_day') => t(w === 'five_hour' ? 'dashboard.forecast.limit_5h' : 'dashboard.forecast.limit_7d');

  const MARK = '{limit}';
  const sentence = $derived.by(() => {
    switch (f.kind) {
      case 'exhaust':
        return t('dashboard.forecast.exhaust', { limit: MARK, when: fmt.when(f.at) });
      case 'lasts':
        return t('dashboard.forecast.lasts', { limit: MARK, when: fmt.when(f.resetsAt) });
      case 'estimate':
        return t('dashboard.forecast.estimate', { pct: fmt.pct(f.pct) });
      case 'none':
        return t('dashboard.forecast.no_reading');
    }
  });

  const support = $derived.by(() => {
    if (f.kind === 'exhaust' || f.kind === 'lasts') {
      const parts: string[] = [];
      if (f.window === 'seven_day' && f.dailyBudgetPct !== null) parts.push(t('dashboard.forecast.daily_budget', { pct: fmt.pct(f.dailyBudgetPct) }));
      if (f.kind === 'exhaust') parts.push(t('dashboard.forecast.reset_note', { when: fmt.when(f.resetsAt) }));
      return parts.join(' ');
    }
    if (f.kind === 'estimate') return t('dashboard.forecast.estimate_body', { count: fmt.number(f.blocks) });
    return t('dashboard.forecast.no_reading_body');
  });
</script>

<section class="hero" class:solo={!limits.five_hour && !limits.seven_day} aria-labelledby="forecast">
  <div class="text">
    <p id="forecast" class="sentence" class:urgent={f.kind === 'exhaust'}>
      {#each sentence.split(MARK) as part, i (i)}{#if i > 0}<span class="nowrap">{limitName(f.kind === 'exhaust' || f.kind === 'lasts' ? f.window : 'seven_day')}</span>{/if}{part}{/each}
    </p>
    {#if support}<p class="support">{support}</p>{/if}
    {#if f.kind === 'estimate'}
      <div class="est" aria-hidden="true"><span style="width:{Math.min(100, f.pct)}%"></span></div>
    {/if}
    {#if f.kind === 'exhaust' || f.kind === 'estimate'}
      <p class="badge"><ConfidenceBadge confidence="estimate" /></p>
    {/if}
  </div>
  {#if limits.five_hour || limits.seven_day}
    <div class="meters">
      <LimitMeter name={t('dashboard.limits.five_hour')} window={limits.five_hour} windowMs={5 * 3_600_000} />
      <LimitMeter name={t('dashboard.limits.seven_day')} window={limits.seven_day} windowMs={7 * 86_400_000} />
    </div>
  {/if}
</section>

<style>
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
    gap: var(--sp-8);
    align-items: center;
    padding: var(--sp-2) 0 var(--sp-7);
  }
  .sentence {
    font-size: var(--fs-3xl);
    font-weight: var(--fw-medium);
    line-height: 1.18;
    letter-spacing: -0.025em;
    max-width: 20ch;
    text-wrap: balance;
  }
  .nowrap {
    white-space: nowrap;
  }
  .support {
    margin-top: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-md);
    max-width: 44ch;
  }
  .badge {
    margin-top: var(--sp-4);
  }
  .solo {
    grid-template-columns: minmax(0, 1fr);
  }
  .solo .sentence {
    max-width: 32ch;
  }
  .est {
    max-width: 520px;
    height: 10px;
    margin-top: var(--sp-4);
    border-radius: var(--r-pill);
    background: var(--meter-track);
  }
  .est span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--meter-fill);
  }
  .meters {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-7);
    padding: var(--sp-6);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
  }
  @media (max-width: 1180px) {
    .hero {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-5);
    }
    .sentence {
      max-width: 28ch;
    }
  }
  @media (max-width: 640px) {
    .meters {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-6);
      padding: var(--sp-5);
    }
    .hero {
      padding-bottom: var(--sp-5);
    }
  }
</style>
