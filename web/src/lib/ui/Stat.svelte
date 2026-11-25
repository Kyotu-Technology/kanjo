<script lang="ts">
  import type { Snippet } from 'svelte';
  import { fmt, t } from '../i18n/index.svelte';
  import Sparkline from '../charts/Sparkline.svelte';

  interface Props {
    label: string;
    value: Snippet;
    /** Change in percent against the reference; null when there is no baseline. */
    delta?: number | null;
    /** Reference phrase, e.g. "last week by this point". */
    reference?: string;
    /** Whether a rise is good news (false for costs). */
    upIsGood?: boolean;
    trend?: number[];
    trendLabel?: string;
    meta?: string;
  }
  let { label, value, delta, reference = '', upIsGood = false, trend, trendLabel, meta }: Props = $props();

  const dir = $derived(delta === null || delta === undefined ? 'none' : Math.abs(delta) < 2 ? 'flat' : delta > 0 ? 'up' : 'down');
  const tone = $derived(dir === 'flat' || dir === 'none' ? 'neutral' : (dir === 'up') === upIsGood ? 'good' : 'bad');
  const deltaText = $derived(
    dir === 'none'
      ? t('dashboard.periods.no_baseline')
      : dir === 'flat'
        ? t('dashboard.periods.flat', { ref: reference })
        : t(dir === 'up' ? 'dashboard.periods.up' : 'dashboard.periods.down', { pct: fmt.pct(Math.abs(delta ?? 0)), ref: reference }),
  );
</script>

<div class="stat">
  <p class="label">{label}</p>
  <div class="value">{@render value()}</div>
  {#if delta !== undefined}
    <p class="delta {tone}">
      {#if dir === 'up' || dir === 'down'}
        <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true" class:down={dir === 'down'}><path d="M5 1.5 9 8H1z" /></svg>
      {/if}
      <span>{deltaText}</span>
    </p>
  {/if}
  {#if meta}<p class="meta">{meta}</p>{/if}
  {#if trend && trend.length > 1}
    <div class="trend"><Sparkline values={trend} label={trendLabel ?? label} /></div>
  {/if}
</div>

<style>
  .stat {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  .label {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .value {
    margin: 2px 0;
  }
  .delta {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .delta svg {
    flex: none;
    fill: currentColor;
    transform: translateY(-1px);
  }
  .delta svg.down {
    transform: rotate(180deg) translateY(1px);
  }
  .bad {
    color: var(--serious-ink);
  }
  .good {
    color: var(--good-ink);
  }
  .bad span,
  .good span {
    color: var(--ink-2);
  }
  .meta {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .trend {
    margin-top: var(--sp-2);
  }
</style>
