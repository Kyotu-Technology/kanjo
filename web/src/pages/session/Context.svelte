<script lang="ts">
  import type { SessionDetail } from '../../lib/api/contract';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/stores/app.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';

  interface Props {
    detail: SessionDetail;
  }
  let { detail }: Props = $props();
  const pct = $derived(detail.status?.ctx_used_pct ?? detail.summary.context_pct);
  const size = $derived(detail.status?.ctx_size ?? 200_000);
  const threshold = $derived(app.settings?.alerts.context_threshold ?? 85);
  const used = $derived(pct === null ? 0 : Math.max(0, Math.min(100, pct)));
  const high = $derived(pct !== null && pct >= threshold);
</script>

<Card title={t('pages.session.ctx.title')}>
  {#snippet actions()}<InlineHelp text={t('pages.session.ctx.help')} label={t('pages.session.ctx.title')} />{/snippet}
  {#if pct === null}
    <p class="muted">{t('pages.session.ctx.none')}</p>
  {:else}
    <p class="big">{t('pages.session.ctx.used', { pct: fmt.pct(pct) })}</p>
    <p class="sub">{t('pages.session.ctx.of', { used: fmt.compact(Math.round((pct / 100) * size)), size: fmt.compact(size) })}</p>
    <div
      class="meter"
      class:high
      role="meter"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(used)}
      aria-label={t('pages.session.ctx.meter', { pct: fmt.pct(pct) })}
    >
      <span class="fill" style="width:{used}%"></span>
      <span class="mark" style="left:{threshold}%" title={t('pages.session.ctx.threshold', { pct: fmt.pct(threshold) })}></span>
    </div>
    <p class="scale"><span>0</span><span>{fmt.compact(size)}</span></p>
    <p class="thl"><span class="tick" aria-hidden="true"></span>{t('pages.session.ctx.threshold', { pct: fmt.pct(threshold) })}</p>
    <p class="state" class:warn={high}>
      <Icon name={high ? 'warning' : 'check'} size={16} />
      {high ? t('pages.session.ctx.high') : t('pages.session.ctx.ok')}
    </p>
    {#if detail.status?.exceeds_200k}<p class="sub">{t('pages.session.ctx.over')}</p>{/if}
  {/if}
</Card>

<style>
  .big {
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
    letter-spacing: var(--tracking-tight);
    font-variant-numeric: tabular-nums;
  }
  .sub,
  .muted {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .meter {
    position: relative;
    height: 10px;
    margin-top: var(--sp-4);
    border-radius: var(--r-pill);
    background: var(--meter-track);
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: inherit;
    background: var(--meter-fill);
  }
  .high .fill {
    background: var(--warning);
  }
  .mark {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 2px;
    margin-left: -1px;
    background: var(--ink-2);
    border-radius: 1px;
  }
  .scale {
    position: relative;
    display: flex;
    justify-content: space-between;
    margin-top: var(--sp-1);
    height: 1.4em;
    color: var(--ink-3);
    font-size: var(--fs-xs);
    font-variant-numeric: tabular-nums;
  }
  .thl {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .tick {
    width: 2px;
    height: 12px;
    border-radius: 1px;
    background: var(--ink-2);
  }
  .state {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
    margin-top: var(--sp-3);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .state :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--good-ink);
  }
  .state.warn :global(svg) {
    color: var(--warning-ink);
  }
</style>
