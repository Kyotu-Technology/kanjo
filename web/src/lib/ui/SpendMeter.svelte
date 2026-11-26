<script lang="ts">
  import type { SpendLimit } from '../api/contract';
  import { fmt, i18n, t, tk } from '../i18n/index.svelte';
  import ConfidenceBadge from './ConfidenceBadge.svelte';
  import Icon from './Icon.svelte';
  import InlineHelp from './InlineHelp.svelte';

  interface Props {
    spend: SpendLimit;
    /** One line and a thin bar, for the dashboard limits card. */
    compact?: boolean;
  }
  let { spend: s, compact = false }: Props = $props();

  const used = $derived(Math.max(0, Math.min(100, s.used_pct)));
  const level = $derived(used >= 90 ? 'critical' : used >= 75 ? 'warning' : 'normal');
  const name = $derived(tk(`pages.limits.spend.period.${s.period === 'day' || s.period === 'week' || s.period === 'month' ? s.period : 'other'}`));
  const amounts = $derived(
    s.used_usd !== null && s.limit_usd !== null ? t('pages.limits.spend.used_of', { used: fmt.money(s.used_usd), limit: fmt.money(s.limit_usd) }) : null,
  );
  const outlook = $derived(
    s.will_exhaust && s.exhausts_at !== null ? t('pages.limits.spend.exhausts', { when: fmt.when(s.exhausts_at) }) : t('pages.limits.spend.lasts'),
  );
</script>

<div class="spend {level}" class:compact>
  <div class="head">
    <h3 class="name">{compact ? t('dashboard.limits.spend') : name}</h3>
    {#if !compact}<InlineHelp text={t('pages.limits.spend.help')} />{/if}
    <span class="badge"><ConfidenceBadge confidence="official" compact={compact} /></span>
  </div>
  {#if compact}
    <p class="line">
      {#if level !== 'normal'}<Icon name="warning" size={15} class="lvl" />{/if}
      <span class="pct">{fmt.pct(used)}</span>
      {#if amounts}<span class="amounts">{amounts}</span>{/if}
      <span class="sep" aria-hidden="true"></span>
      <span class="reset">{t('pages.limits.spend.resets', { when: fmt.when(s.resets_at) })}</span>
    </p>
  {:else}
    <p class="figure">
      {#if level !== 'normal'}<Icon name={level === 'critical' ? 'critical' : 'warning'} size={20} class="lvl" />{/if}
      <span class="big">{fmt.pct(used)}</span>
      <span class="used">{t('dashboard.limits.used')}</span>
    </p>
    {#if amounts}<p class="amounts">{amounts}</p>{/if}
  {/if}
  <div
    class="track"
    role="meter"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={Math.round(used)}
    aria-valuetext={t('pages.limits.spend.meter', { pct: fmt.pct(used) })}
    aria-label={name}
  >
    <span class="fill" style="width:{used}%"></span>
  </div>
  {#if !compact}
    <div class="foot">
      <span>{t('dashboard.limits.resets_in', { duration: fmt.duration(s.resets_at - i18n.now) })}</span>
    </div>
    <p class="outlook" class:runs-out={s.will_exhaust}>
      {#if s.will_exhaust}<Icon name="warning" size={15} />{/if}{outlook}
    </p>
    <p class="body">{t('pages.limits.spend.body')}</p>
  {:else if s.will_exhaust && s.exhausts_at !== null}
    <p class="outlook runs-out"><Icon name="warning" size={14} />{outlook}</p>
  {/if}
</div>

<style>
  .spend {
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
  .compact {
    grid-column: 1 / -1;
    padding-top: var(--sp-5);
    border-top: 1px solid var(--line);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
  }
  .badge {
    margin-left: auto;
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
    margin: var(--sp-2) 0 var(--sp-1);
  }
  .figure :global(.lvl),
  .line :global(.lvl) {
    align-self: center;
  }
  .warning :global(.lvl) {
    color: var(--warning-ink);
  }
  .critical :global(.lvl) {
    color: var(--critical-ink);
  }
  .big {
    font-size: var(--fs-2xl);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.02em;
    line-height: 1;
  }
  .used {
    color: var(--ink-2);
  }
  .amounts {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
    margin-bottom: var(--sp-3);
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--sp-1) var(--sp-3);
    margin: var(--sp-1) 0 var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .line .pct {
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
    color: var(--ink);
  }
  .line .amounts {
    margin: 0;
  }
  .sep {
    flex: 1;
  }
  .track {
    position: relative;
    height: 12px;
    border-radius: var(--r-pill);
    background: var(--track);
  }
  .compact .track {
    height: 6px;
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: inherit;
    background: var(--fill);
    min-width: 6px;
    transition: width 600ms var(--ease);
  }
  .foot {
    margin-top: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .outlook {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: var(--sp-1);
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .outlook.runs-out {
    font-weight: var(--fw-medium);
    color: var(--serious-ink);
  }
  .body {
    margin-top: var(--sp-3);
    color: var(--ink-3);
    font-size: var(--fs-sm);
    max-width: 46ch;
  }
</style>
