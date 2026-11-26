<script lang="ts">
  import type { LimitEstimate } from '../../lib/api/contract';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import Money from '../../lib/ui/Money.svelte';

  interface Props {
    estimate: LimitEstimate;
    active: boolean;
    refreshing?: boolean;
  }
  let { estimate: e, active, refreshing = false }: Props = $props();
  const pct = $derived(Math.max(0, Math.min(100, e.current_block_pct)));
</script>

<Card title={t('pages.limits.estimate.title')} {refreshing}>
  {#snippet actions()}<ConfidenceBadge confidence={e.confidence} />{/snippet}
  {#if active}
    <p class="sentence">{t('pages.limits.estimate.sentence', { pct: fmt.pct(pct) })}</p>
    <div
      class="track"
      role="meter"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={Math.round(pct)}
      aria-valuetext={fmt.pct(pct)}
      aria-label={t('pages.limits.estimate.title')}
    >
      <span class="fill" style="width:{pct}%"></span>
    </div>
  {:else}
    <p class="sentence quiet">{t('pages.limits.estimate.idle')}</p>
  {/if}
  <dl class="facts">
    <div>
      <dt>{t('pages.limits.estimate.so_far')}</dt>
      <dd><Money usd={e.current_block_usd} confidence="lower_bound" size="lg" /></dd>
    </div>
    <div>
      <dt>{t('pages.limits.estimate.ceiling')}</dt>
      <dd><Money usd={e.p90_block_usd} confidence="estimate" size="lg" /></dd>
    </div>
  </dl>
  <p class="body">
    {t('pages.limits.estimate.body', { current: fmt.money(e.current_block_usd), count: fmt.number(e.blocks_observed), p90: fmt.money(e.p90_block_usd) })}
  </p>
</Card>

<style>
  .sentence {
    font-size: var(--fs-xl);
    font-weight: var(--fw-medium);
    line-height: var(--lh-snug);
    letter-spacing: -0.015em;
    max-width: 34ch;
    text-wrap: balance;
  }
  .quiet {
    color: var(--ink-2);
  }
  .track {
    position: relative;
    height: 12px;
    margin-top: var(--sp-4);
    border-radius: var(--r-pill);
    background: var(--meter-track);
    background-image: repeating-linear-gradient(135deg, transparent 0 5px, var(--surface) 5px 7px);
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    min-width: 12px;
    border-radius: inherit;
    background: var(--meter-fill);
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-4) var(--sp-8);
    margin-top: var(--sp-5);
  }
  dt {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    margin-bottom: var(--sp-1);
  }
  .body {
    margin-top: var(--sp-4);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 62ch;
  }
</style>
