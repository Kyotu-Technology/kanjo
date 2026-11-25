<script lang="ts">
  import type { Overview } from '../../lib/api/contract';
  import { t, tn } from '../../lib/i18n/index.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import Stat from '../../lib/ui/Stat.svelte';

  interface Props {
    overview: Overview;
  }
  let { overview: o }: Props = $props();
  const conf = $derived(o.data.cost_confidence);
  const periods = $derived([
    { key: 'today', label: t('dashboard.periods.today'), p: o.today, ref: t('dashboard.periods.ref_today'), trend: cumulative(o.hourly.filter((b) => b.start >= o.today.from).map((b) => b.usage.cost_usd)) },
    { key: 'week', label: t('dashboard.periods.week'), p: o.week, ref: t('dashboard.periods.ref_week'), trend: o.daily.slice(-7).map((b) => b.usage.cost_usd) },
    { key: 'month', label: t('dashboard.periods.month'), p: o.month, ref: t('dashboard.periods.ref_month'), trend: o.daily.map((b) => b.usage.cost_usd) },
  ]);

  function cumulative(v: number[]): number[] {
    let s = 0;
    return v.map((x) => (s += x));
  }
</script>

<section class="periods" aria-label={t('dashboard.periods.label')}>
  {#each periods as item (item.key)}
    <div class="cell">
      <Stat label={item.label} delta={item.p.delta_pct} reference={item.ref} trend={item.trend} meta={tn('dashboard.periods.requests', item.p.usage.requests)}>
        {#snippet value()}
          <Money usd={item.p.usage.cost_usd} confidence={conf} unpriced={item.p.usage.unpriced_requests} size="lg" />
        {/snippet}
      </Stat>
    </div>
  {/each}
</section>

<style>
  .periods {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
  }
  .cell {
    padding: var(--sp-5) var(--sp-6);
    min-width: 0;
  }
  .cell + .cell {
    border-left: 1px solid var(--line);
  }
  @media (max-width: 860px) {
    .periods {
      grid-template-columns: minmax(0, 1fr);
    }
    .cell + .cell {
      border-left: 0;
      border-top: 1px solid var(--line);
    }
    .cell {
      padding: var(--sp-4) var(--sp-5);
    }
  }
</style>
