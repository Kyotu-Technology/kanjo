<script lang="ts">
  import type { ModelMixSuggestion } from '../../lib/api/contract';
  import { modelColor, modelLabel, modelTitle } from '../../lib/charts/series';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import NextStep from './NextStep.svelte';

  interface Props {
    items: ModelMixSuggestion[];
    refreshing?: boolean;
  }
  let { items, refreshing = false }: Props = $props();
  const kind = (code: string) => (code.startsWith('subagents_to_') ? 'subagents' : code.startsWith('short_turns_to_') ? 'short_turns' : 'other');
  const params = (m: ModelMixSuggestion) => ({
    scope: m.scope === 'subagent' || m.scope === 'main' ? '' : m.scope,
    from: modelLabel(m.from_model),
    to: modelLabel(m.to_model),
    requests: fmt.number(m.affected_requests),
    current: fmt.money(m.current_usd),
    projected: fmt.money(m.projected_usd),
  });
  const clean = (s: string) => s.replace(/\s+/g, ' ').trim();
  const total = $derived(items.reduce((a, m) => a + m.saving_usd, 0));
</script>

<Card title={t('pages.insights.mix.title')} subtitle={t('pages.insights.mix.subtitle')} {refreshing}>
  {#if items.length}
    {#if items.length > 1}<p class="total">{t('pages.insights.mix.total', { value: fmt.money(total) })} <ConfidenceBadge confidence="estimate" compact /></p>{/if}
    <ul class="list">
      {#each items as m (m.code + m.scope)}
        {@const p = params(m)}
        <li>
          <div class="head">
            <h3>{clean(tk(`pages.insights.mix.name.${kind(m.code)}`, p))}</h3>
            <span class="models" aria-hidden="true">
              <span class="dot" style="background:{modelColor(m.from_model)}"></span><span title={modelTitle(m.from_model)}>{p.from}</span>
              <span class="to">/</span>
              <span class="dot" style="background:{modelColor(m.to_model)}"></span>{p.to}
            </span>
          </div>
          <p class="body">{tk(`pages.insights.mix.body.${kind(m.code)}`, p)}</p>
          <p class="saving"><span>{t('pages.insights.mix.saving')}</span><Money usd={m.saving_usd} confidence={m.confidence} size="lg" /></p>
          <NextStep text={clean(tk(`pages.insights.mix.do.${kind(m.code)}`, { ...p, scope: p.scope || t('pages.insights.mix.subagent') }))} />
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState compact icon="check" title={t('pages.insights.mix.none')} />
  {/if}
</Card>

<style>
  .total {
    margin-bottom: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .list li {
    padding: var(--sp-4) 0;
  }
  .list li:first-child {
    padding-top: 0;
  }
  .list li + li {
    border-top: 1px solid var(--line);
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--sp-1) var(--sp-3);
  }
  h3 {
    font-size: var(--fs-md);
    font-weight: var(--fw-semibold);
    letter-spacing: 0;
  }
  .models {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .to {
    margin: 0 2px;
  }
  .body {
    margin-top: var(--sp-1);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 64ch;
  }
  .saving {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--sp-2);
    margin-top: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
</style>
