<script lang="ts">
  import type { Confidence } from '../api/contract';
  import { fmt, t, tn } from '../i18n/index.svelte';
  import ConfidenceBadge from './ConfidenceBadge.svelte';
  import Tip from './Tip.svelte';

  interface Props {
    /** Amount in USD; converted with the display currency from Settings. */
    usd: number | null | undefined;
    /** Required: every money figure states how far it can be trusted. */
    confidence: Confidence;
    /** Requests left out of the amount because their model has no price. */
    unpriced?: number;
    size?: 'sm' | 'md' | 'lg' | 'xl';
    compact?: boolean;
    /** Badge beside the amount; 'seal' for dense rows, 'none' only when a column header carries it. */
    badge?: 'full' | 'seal' | 'none';
  }

  let { usd, confidence, unpriced = 0, size = 'md', compact = false, badge = 'full' }: Props = $props();
  const text = $derived(usd === null || usd === undefined ? t('common.unknown') : fmt.money(usd, { compact }));
</script>

<span class="money {size}">
  <span class="amount">{text}</span>
  {#if unpriced > 0}
    <Tip text={tn('confidence.unpriced', unpriced)} label={tn('confidence.unpriced', unpriced)}>
      <span class="unpriced" aria-hidden="true">+</span>
    </Tip>
  {/if}
  {#if badge === 'full'}
    <ConfidenceBadge {confidence} />
  {:else if badge === 'seal'}
    <ConfidenceBadge {confidence} compact />
  {:else}
    <span class="sr-only">{t(`confidence.${confidence}`)}</span>
  {/if}
</span>

<style>
  .money {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  .amount {
    font-weight: var(--fw-semibold);
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .sm .amount {
    font-weight: var(--fw-medium);
    font-variant-numeric: tabular-nums;
  }
  .md .amount {
    font-size: var(--fs-md);
  }
  .lg .amount {
    font-size: var(--fs-2xl);
    line-height: var(--lh-tight);
  }
  .xl .amount {
    font-size: var(--fs-hero);
    line-height: 1;
    letter-spacing: -0.03em;
  }
  .unpriced {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--warning-soft);
    color: var(--warning-ink);
    font-size: var(--fs-sm);
    font-weight: var(--fw-bold);
  }
</style>
