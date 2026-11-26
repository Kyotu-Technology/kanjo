<script lang="ts">
  import type { Dimension, Driver } from '../../lib/api/contract';
  import { queries } from '../../lib/api/queries';
  import { useQuery } from '../../lib/api/query.svelte';
  import { modelColor } from '../../lib/charts/series';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { describeError } from '../../lib/stores/app.svelte';
  import Banner from '../../lib/ui/Banner.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';
  import Tabs from '../../lib/ui/Tabs.svelte';
  import { rowLabel } from './labels';

  interface Props {
    /** True when the page has filters the comparison does not apply. */
    filtered?: boolean;
  }
  let { filtered = false }: Props = $props();
  let period = $state<'week' | 'month'>('week');
  const cmp = useQuery(() => queries.compare(period));
  const c = $derived(cmp.data);
  const DIMS: { dim: Dimension; key: 'by_project' | 'by_model' | 'by_agent' }[] = [
    { dim: 'project', key: 'by_project' },
    { dim: 'model', key: 'by_model' },
    { dim: 'agent', key: 'by_agent' },
  ];
  const NOISE = 0.005;
  const drivers = $derived((c?.drivers ?? []).filter((d) => Math.abs(d.delta_usd) >= NOISE));
  const maxAbs = $derived(Math.max(...drivers.map((d) => Math.abs(d.delta_usd)), 1e-9));
  const groups = $derived(DIMS.map((g) => ({ ...g, rows: drivers.filter((d) => d.dimension === g.dim).slice(0, 4) })));
  const dir = $derived.by(() => {
    if (!c) return 'none';
    if (c.delta_pct === null) return 'no_base';
    return Math.abs(c.delta_pct) < 2 ? 'flat' : c.delta_usd > 0 ? 'up' : 'down';
  });
  const top = $derived.by(() => {
    if (dir !== 'up' && dir !== 'down') return null;
    const same = drivers.filter((d) => (dir === 'up' ? d.delta_usd > 0 : d.delta_usd < 0));
    return same.sort((a, b) => Math.abs(b.delta_usd) - Math.abs(a.delta_usd))[0] ?? null;
  });
  const span = (from: number, to: number) => `${fmt.date(from, 'short')} - ${fmt.date(to - 1, 'short')}`;
  const signed = (v: number) => `${v > 0 ? '+' : v < 0 ? '-' : ''}${fmt.money(Math.abs(v))}`;
  const label = (d: Driver) => rowLabel(d.dimension, d);
</script>

<Card title={t('pages.usage.compare.title')} refreshing={cmp.refreshing}>
  <div class="switch">
    <Tabs
      size="sm"
      label={t('pages.usage.compare.title')}
      items={[
        { id: 'week', label: t('pages.usage.compare.week') },
        { id: 'month', label: t('pages.usage.compare.month') },
      ]}
      bind:value={period}
    />
  </div>
  {#if cmp.error && !c}
    <Banner tone="warning" title={t('common.error_title')} body={describeError(cmp.error)}>
      {#snippet action()}<Button size="sm" icon="refresh" onclick={() => cmp.refetch()}>{t('common.retry')}</Button>{/snippet}
    </Banner>
  {:else if !c}
    <Skeleton lines={5} height="1.3em" />
  {:else}
    {@const p = { period: t(`pages.usage.compare.cur_${period}`), previous: t(`pages.usage.compare.prev_${period}`), current: fmt.money(c.current.usage.cost_usd) }}
    <div class="cols">
    <div>
    <p class="lead">
      {#if dir === 'up' || dir === 'down'}
        {t(`pages.usage.compare.${dir}`, { ...p, delta: fmt.money(Math.abs(c.delta_usd)), pct: fmt.pct(Math.abs(c.delta_pct ?? 0)) })}
      {:else if dir === 'flat'}{t('pages.usage.compare.flat', p)}{:else}{t('pages.usage.compare.no_base', p)}{/if}
      {#if top}
        {t(dir === 'up' ? 'pages.usage.compare.top_up' : 'pages.usage.compare.top_down', { label: label(top), delta: fmt.money(Math.abs(top.delta_usd)) })}
      {/if}
    </p>
    <div class="sides">
      <div>
        <span class="k">{t('pages.usage.compare.now')}, {span(c.current.from, c.current.to)}</span>
        <Money usd={c.current.usage.cost_usd} confidence="lower_bound" unpriced={c.current.usage.unpriced_requests} size="md" badge="none" />
      </div>
      <div>
        <span class="k">{t('pages.usage.compare.before')}, {span(c.previous.from, c.previous.to)}</span>
        <Money usd={c.previous.usage.cost_usd} confidence="lower_bound" unpriced={c.previous.usage.unpriced_requests} size="md" badge="none" />
      </div>
      <ConfidenceBadge confidence="lower_bound" />
    </div>
    </div>
    <div>
    <h3 class="dh">{t('pages.usage.compare.drivers')}</h3>
    {#if !drivers.length}
      <p class="muted">{t('pages.usage.compare.none')}</p>
    {:else}
      <div class="groups">
        {#each groups as g (g.dim)}
          <section aria-labelledby="drv-{g.dim}">
            <h4 id="drv-{g.dim}">{t(`pages.usage.compare.${g.key}`)}</h4>
            {#if g.rows.length}
              <ul>
                {#each g.rows as d (d.key)}
                  <li>
                    <div class="line">
                      <span class="name">
                        {#if g.dim === 'model'}<span class="dot" style="background:{modelColor(d.key)}"></span>{/if}
                        {label(d)}
                      </span>
                      <strong class="delta">{signed(d.delta_usd)}</strong>
                    </div>
                    <div class="bar" aria-hidden="true">
                      <span class:neg={d.delta_usd < 0} style="width:{(Math.abs(d.delta_usd) / maxAbs) * 50}%"></span>
                    </div>
                    <span class="was">
                      {#if d.previous_usd < NOISE}{t('pages.usage.compare.new')}{:else if d.current_usd < NOISE}{t('pages.usage.compare.gone')}{:else}{t('pages.usage.compare.was', { value: fmt.money(d.previous_usd) })}{/if}
                    </span>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="muted">{t('pages.usage.compare.none')}</p>
            {/if}
          </section>
        {/each}
      </div>
      <p class="foot">{t('pages.usage.compare.drivers_note')}{#if filtered}{' '}{t('pages.usage.compare.unfiltered')}{/if}</p>
    {/if}
    </div>
    </div>
  {/if}
</Card>

<style>
  .switch {
    margin: calc(-1 * var(--sp-1)) 0 var(--sp-4);
    max-width: 100%;
    overflow-x: auto;
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 4fr) minmax(0, 8fr);
    gap: var(--sp-5) var(--sp-8);
  }
  @media (max-width: 1000px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .lead {
    font-size: var(--fs-base);
    color: var(--ink);
    max-width: 70ch;
    text-wrap: pretty;
  }
  .sides {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--sp-3) var(--sp-7);
    margin-top: var(--sp-4);
  }
  .sides > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .k {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .dh {
    margin-top: 0;
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 160px), 1fr));
    gap: var(--sp-4) var(--sp-6);
    margin-top: var(--sp-3);
  }
  h4 {
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
    color: var(--ink-3);
    padding-bottom: var(--sp-2);
    border-bottom: 1px solid var(--line);
    margin-bottom: var(--sp-2);
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  .line {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--sp-2);
    font-size: var(--fs-sm);
  }
  .name {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex: none;
  }
  .delta {
    font-variant-numeric: tabular-nums;
    font-weight: var(--fw-semibold);
    flex: none;
  }
  .bar {
    position: relative;
    height: 6px;
    margin-top: 5px;
    border-radius: var(--r-pill);
    background: var(--meter-track);
  }
  .bar::after {
    content: '';
    position: absolute;
    left: 50%;
    top: -2px;
    bottom: -2px;
    width: 1px;
    background: var(--axis);
  }
  .bar span {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    border-radius: 0 var(--r-pill) var(--r-pill) 0;
    background: var(--meter-fill);
  }
  .bar span.neg {
    left: auto;
    right: 50%;
    border-radius: var(--r-pill) 0 0 var(--r-pill);
  }
  .was {
    display: block;
    margin-top: 3px;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .muted {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .foot {
    margin-top: var(--sp-4);
    color: var(--ink-3);
    font-size: var(--fs-xs);
    max-width: 80ch;
  }
</style>
