<script lang="ts">
  import type { ContextWarning } from '../../lib/api/contract';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import Card from '../../lib/ui/Card.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import NextStep from './NextStep.svelte';

  interface Props {
    items: ContextWarning[];
    refreshing?: boolean;
  }
  let { items, refreshing = false }: Props = $props();
  const level = (pct: number) => (pct >= 90 ? 'critical' : 'warning');
</script>

<Card title={t('pages.insights.context.title')} {refreshing}>
  {#snippet actions()}<InlineHelp text={t('pages.insights.context.help')} />{/snippet}
  <p class="intro">{t('pages.insights.context.body')}</p>
  {#if items.length}
    <ul class="list">
      {#each items as c (c.session_id)}
        <li class={level(c.ctx_used_pct)}>
          <div class="head">
            <a href={href(`/sessions/${encodeURIComponent(c.session_id)}`)}>{c.project}</a>
            <span class="when">{fmt.ago(c.ts)}</span>
          </div>
          <p class="fig">
            <Icon name={level(c.ctx_used_pct) === 'critical' ? 'critical' : 'warning'} size={16} class="lvl" />
            {t('pages.insights.context.row', { pct: fmt.pct(c.ctx_used_pct), size: fmt.number(c.ctx_size ?? 200_000) })}
          </p>
          <div
            class="track"
            role="meter"
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={Math.round(c.ctx_used_pct)}
            aria-valuetext={t('pages.insights.context.meter', { pct: fmt.pct(c.ctx_used_pct) })}
            aria-label={c.project}
          >
            <span style="width:{Math.min(100, c.ctx_used_pct)}%"></span>
          </div>
          {#if c.exceeds_200k}<p class="over">{t('pages.insights.context.over')}</p>{/if}
        </li>
      {/each}
    </ul>
    <NextStep text={t('pages.insights.context.do')} />
  {:else}
    <EmptyState compact icon="check" title={t('pages.insights.context.none')} />
  {/if}
</Card>

<style>
  .intro {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 56ch;
    margin-bottom: var(--sp-3);
  }
  .list li {
    padding: var(--sp-3) 0;
  }
  .list li + li {
    border-top: 1px solid var(--line);
  }
  .head {
    display: flex;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  .head a {
    font-weight: var(--fw-semibold);
  }
  .when {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .fig {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: var(--sp-1) 0 var(--sp-2);
    font-size: var(--fs-sm);
  }
  .warning :global(.lvl) {
    color: var(--warning-ink);
  }
  .critical :global(.lvl) {
    color: var(--critical-ink);
  }
  .track {
    height: 8px;
    border-radius: var(--r-pill);
    background: var(--warning-soft);
  }
  .critical .track {
    background: var(--critical-soft);
  }
  .track span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--warning);
  }
  .critical .track span {
    background: var(--critical);
  }
  .over {
    margin-top: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
</style>
