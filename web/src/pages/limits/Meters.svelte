<script lang="ts">
  import type { LimitWindow, Limits } from '../../lib/api/contract';
  import { t } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import LimitMeter from '../../lib/ui/LimitMeter.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import SpendMeter from '../../lib/ui/SpendMeter.svelte';

  interface Props {
    limits: Limits;
    refreshing?: boolean;
  }
  let { limits, refreshing = false }: Props = $props();
  const rows = $derived<{ key: string; name: string; w: LimitWindow | null; ms: number }[]>(
    limits.spend && !limits.five_hour && !limits.seven_day
      ? []
      : [
          { key: '5h', name: t('dashboard.limits.five_hour'), w: limits.five_hour, ms: 5 * 3_600_000 },
          { key: '7d', name: t('dashboard.limits.seven_day'), w: limits.seven_day, ms: 7 * 86_400_000 },
        ],
  );
  const cols = $derived(rows.length + (limits.spend ? 1 : 0));
</script>

<Card title={t('dashboard.limits.title')} {refreshing}>
  <div class="meters" class:three={cols === 3} class:one={cols === 1}>
    {#each rows as r (r.key)}
      <div class="col">
        <LimitMeter name={r.name} window={r.w} windowMs={r.ms} />
        {#if r.w}
          <dl class="facts">
            <div>
              <dt>{t('pages.limits.in_window')}<InlineHelp text={t('pages.limits.in_window_help')} /></dt>
              <dd><Money usd={r.w.usd_in_window} confidence="lower_bound" size="sm" badge="seal" /></dd>
            </div>
            {#if r.w.usd_per_pct !== null}
              <div>
                <dt>{t('pages.limits.per_pct')}</dt>
                <dd><Money usd={r.w.usd_per_pct} confidence="estimate" size="sm" badge="seal" /></dd>
              </div>
            {/if}
          </dl>
        {/if}
      </div>
    {/each}
    {#if limits.spend}<div class="col"><SpendMeter spend={limits.spend} /></div>{/if}
  </div>
</Card>

<style>
  .meters {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-7);
  }
  .meters.three {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  .meters.one {
    grid-template-columns: minmax(0, 460px);
  }
  .col {
    min-width: 0;
  }
  .facts {
    display: grid;
    gap: var(--sp-2);
    margin-top: var(--sp-4);
    padding-top: var(--sp-3);
    border-top: 1px solid var(--line);
  }
  .facts div {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  dt {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  @media (max-width: 1180px) {
    .meters.three {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 640px) {
    .meters,
    .meters.three {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-6);
    }
  }
</style>
