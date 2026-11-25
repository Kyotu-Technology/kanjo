<script lang="ts">
  import type { Block, BurnRate, Confidence } from '../../lib/api/contract';
  import { modelColor, modelLabel } from '../../lib/charts/series';
  import { fmt, i18n, t } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';

  interface Props {
    block: Block | null;
    burn: BurnRate;
    confidence: Confidence;
    refreshing?: boolean;
  }
  let { block, burn, confidence, refreshing = false }: Props = $props();
  const elapsed = $derived(block ? Math.max(0, Math.min(1, (i18n.now - block.start) / (block.end - block.start))) : 0);
  const total = $derived(block ? block.by_model.reduce((a, r) => a + r.usage.cost_usd, 0) : 0);
</script>

<Card title={t('dashboard.block.title')} {refreshing} class="block-card">
  {#if block}
    <p class="when">{t('dashboard.block.started', { time: fmt.time(block.start) })}. {t('dashboard.block.ends', { when: fmt.when(block.end) })}</p>
    <div class="timebar" aria-hidden="true"><span style="width:{elapsed * 100}%"></span></div>
    <div class="spent">
      <p class="lbl">{t('dashboard.block.spent')}</p>
      <Money usd={block.usage.cost_usd} {confidence} unpriced={block.usage.unpriced_requests} size="lg" />
    </div>
    <dl class="pair">
      <div>
        <dt><Icon name="bolt" size={15} />{t('dashboard.block.burn')}</dt>
        <dd>
          {#if burn.active}
            <Money usd={block.burn_usd_per_hour ?? burn.usd_per_hour} confidence="estimate" badge="none" />
            <span class="unit">{t('dashboard.block.per_hour')}</span>
            <ConfidenceBadge confidence="estimate" compact />
          {:else}
            <span class="quiet">{t('dashboard.block.idle', { minutes: burn.window_minutes })}</span>
          {/if}
        </dd>
      </div>
      <div>
        <dt><Icon name="clock" size={15} />{t('dashboard.block.projected')}</dt>
        <dd>
          <Money usd={block.projected_usd ?? block.usage.cost_usd} confidence="estimate" badge="seal" />
        </dd>
        <p class="note">{t('dashboard.block.projected_note')}</p>
      </div>
    </dl>
    {#if block.by_model.length}
      <p class="lbl models-title">{t('dashboard.block.by_model')} <ConfidenceBadge {confidence} compact /></p>
      <div class="seg" role="img" aria-label={block.by_model.map((r) => `${modelLabel(r.key)} ${fmt.money(r.usage.cost_usd)}`).join(', ')}>
        {#each block.by_model as r (r.key)}
          {#if r.usage.cost_usd > 0}<span style="flex:{r.usage.cost_usd / (total || 1)};background:{modelColor(r.key)}"></span>{/if}
        {/each}
      </div>
      <ul class="models">
        {#each block.by_model as r (r.key)}
          <li><span class="key" style="background:{modelColor(r.key)}"></span>{modelLabel(r.key)}<span class="v">{fmt.money(r.usage.cost_usd)}</span></li>
        {/each}
      </ul>
    {/if}
  {:else}
    <EmptyState compact icon="clock" title={t('dashboard.block.none')} />
  {/if}
</Card>

<style>
  .when {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .timebar {
    height: 4px;
    margin: var(--sp-2) 0 var(--sp-4);
    border-radius: var(--r-pill);
    background: var(--surface-3);
  }
  .timebar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--ink-3);
  }
  .lbl,
  dt {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .spent {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-4);
    margin-top: var(--sp-5);
    padding-top: var(--sp-4);
    border-top: 1px solid var(--line);
  }
  dt {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  dt :global(svg) {
    color: var(--ink-3);
  }
  dd {
    display: flex;
    align-items: baseline;
    gap: 4px;
    flex-wrap: wrap;
    margin-top: var(--sp-1);
  }
  .unit {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .quiet {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .note {
    color: var(--ink-3);
    font-size: var(--fs-xs);
    margin-top: 2px;
  }
  .models-title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: var(--sp-5);
  }
  .seg {
    display: flex;
    gap: 2px;
    height: 8px;
    margin: var(--sp-2) 0 var(--sp-3);
  }
  .seg span {
    border-radius: var(--r-pill);
    min-width: 4px;
  }
  .models {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--fs-sm);
  }
  .models li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .key {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .v {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
    font-weight: var(--fw-medium);
  }
</style>
