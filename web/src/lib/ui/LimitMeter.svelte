<script lang="ts">
  import type { LimitWindow } from '../api/contract';
  import { fmt, i18n, t } from '../i18n/index.svelte';
  import ConfidenceBadge from './ConfidenceBadge.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    name: string;
    window: LimitWindow | null;
    /** Length of the window, to show how much of the time has passed. */
    windowMs: number;
  }
  let { name, window: w, windowMs }: Props = $props();

  const used = $derived(w ? Math.max(0, Math.min(100, w.used_pct)) : 0);
  const elapsed = $derived(w ? Math.max(0, Math.min(100, ((windowMs - (w.resets_at - i18n.now)) / windowMs) * 100)) : 0);
  const level = $derived(used >= 90 ? 'critical' : used >= 75 ? 'warning' : 'normal');
  const pace = $derived(!w || elapsed < 3 ? null : used > elapsed * 1.1 ? 'ahead' : used < elapsed * 0.9 ? 'behind' : 'even');
  const paceText = $derived(pace === 'ahead' ? t('dashboard.limits.pace_ahead') : pace === 'behind' ? t('dashboard.limits.pace_behind') : t('dashboard.limits.pace_even'));
</script>

<div class="meter {level}">
  <div class="head">
    <h3 class="name">{name}</h3>
    {#if w}<ConfidenceBadge confidence={w.confidence} />{/if}
  </div>
  {#if w}
    <p class="figure">
      {#if level !== 'normal'}<Icon name={level === 'critical' ? 'critical' : 'warning'} size={20} class="lvl" />{/if}
      <span class="pct">{fmt.pct(used)}</span>
      <span class="used">{t('dashboard.limits.used')}</span>
    </p>
    <div
      class="track"
      role="meter"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={Math.round(used)}
      aria-valuetext={t('dashboard.limits.meter_label', { name, pct: fmt.pct(used) })}
      aria-label={name}
    >
      <span class="fill" style="width:{used}%"></span>
      <span class="clock" style="left:{elapsed}%" title={t('dashboard.limits.elapsed', { pct: fmt.pct(elapsed) })}></span>
    </div>
    <div class="ruler" aria-hidden="true">
      {#each [0, 25, 50, 75, 100] as tick (tick)}<span style="left:{tick}%"></span>{/each}
    </div>
    <div class="foot">
      <span>{t('dashboard.limits.resets_in', { duration: fmt.duration(w.resets_at - i18n.now) })}</span>
      <span class="clock-label"><span class="clock-key" aria-hidden="true"></span>{t('dashboard.limits.elapsed', { pct: fmt.pct(elapsed) })}</span>
    </div>
    {#if w.will_exhaust && w.exhausts_at !== null}
      <p class="runs-out"><Icon name="warning" size={15} />{t('dashboard.limits.exhausts', { when: fmt.when(w.exhausts_at) })}</p>
    {:else if pace}
      <p class="pace">{paceText}</p>
    {/if}
    {#if w.stale}<p class="stale"><Icon name="clock" size={14} />{t('dashboard.limits.stale', { ago: fmt.ago(w.observed_at) })}</p>{/if}
  {:else}
    <p class="none">{t('dashboard.limits.unavailable')}</p>
  {/if}
</div>

<style>
  .meter {
    --fill: var(--meter-fill);
    --track: var(--meter-track);
    min-width: 0;
  }
  .warning {
    --fill: var(--warning);
    --track: var(--warning-soft);
  }
  .critical {
    --fill: var(--critical);
    --track: var(--critical-soft);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-2);
  }
  .name {
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
    color: var(--ink-2);
    letter-spacing: 0;
  }
  .figure {
    display: flex;
    align-items: baseline;
    gap: var(--sp-2);
    margin: var(--sp-2) 0 var(--sp-3);
  }
  .figure :global(.lvl) {
    align-self: center;
  }
  .warning .figure :global(.lvl) {
    color: var(--warning-ink);
  }
  .critical .figure :global(.lvl) {
    color: var(--critical-ink);
  }
  .pct {
    font-size: var(--fs-2xl);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.02em;
    line-height: 1;
  }
  .used {
    color: var(--ink-2);
  }
  .track {
    position: relative;
    height: 12px;
    border-radius: var(--r-pill);
    background: var(--track);
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: inherit;
    background: var(--fill);
    min-width: 12px;
    transition: width 600ms var(--ease);
  }
  .clock {
    position: absolute;
    top: -5px;
    bottom: -5px;
    width: 2px;
    margin-left: -1px;
    background: var(--ink);
    border-radius: 1px;
    box-shadow: 0 0 0 2px var(--surface);
  }
  .ruler {
    position: relative;
    height: 6px;
    margin-top: 3px;
  }
  .ruler span {
    position: absolute;
    top: 0;
    width: 1px;
    height: 5px;
    background: var(--axis);
  }
  .ruler span:last-child {
    margin-left: -1px;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--sp-1) var(--sp-3);
    margin-top: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .clock-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .clock-key {
    width: 2px;
    height: 12px;
    background: var(--ink);
    border-radius: 1px;
  }
  .pace {
    margin-top: var(--sp-1);
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .runs-out {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: var(--sp-1);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    color: var(--serious-ink);
  }
  .stale {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--warning-ink);
  }
  .none {
    margin-top: var(--sp-2);
    color: var(--ink-3);
  }
</style>
