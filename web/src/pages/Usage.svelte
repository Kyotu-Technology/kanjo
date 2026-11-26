<script lang="ts">
  import type { BreakdownRow } from '../lib/api/contract';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { modelLabel } from '../lib/charts/series';
  import { fmt, i18n, t, tk, tn } from '../lib/i18n/index.svelte';
  import { href } from '../lib/router';
  import { app, describeError } from '../lib/stores/app.svelte';
  import { router } from '../lib/stores/router.svelte';
  import Answer from '../lib/ui/Answer.svelte';
  import Rich, { strong } from '../lib/ui/Rich.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Card from '../lib/ui/Card.svelte';
  import DateRangePicker from '../lib/ui/DateRangePicker.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import InlineHelp from '../lib/ui/InlineHelp.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Select from '../lib/ui/Select.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import Stat from '../lib/ui/Stat.svelte';
  import Tabs from '../lib/ui/Tabs.svelte';
  import BreakdownCard from './usage/BreakdownCard.svelte';
  import Compare from './usage/Compare.svelte';
  import ExportDialog from './usage/ExportDialog.svelte';
  import { usePageExport } from '../lib/shell/exports.svelte';
  import Figure from './usage/Figure.svelte';
  import FigureStrip from './usage/FigureStrip.svelte';
  import Heatmaps from './usage/Heatmaps.svelte';
  import { rangeText, rowLabel } from './usage/labels';
  import OverTime from './usage/OverTime.svelte';
  import { apiFilters, fromQuery, toQuery, type UsageFilters } from './usage/state';

  let f = $state<UsageFilters>(fromQuery(router.current.query, i18n.now));
  let exportOpen = $state(false);
  let lastQuery = JSON.stringify(router.current.query);

  $effect(() => {
    const q = router.current.query;
    const key = JSON.stringify(q);
    if (key === lastQuery) return;
    lastQuery = key;
    f = fromQuery(q, i18n.now);
  });
  $effect(() => {
    const target = href('/usage', toQuery(f));
    if (location.hash !== target) {
      history.replaceState(history.state, '', target);
      lastQuery = JSON.stringify(Object.fromEntries(new URLSearchParams(target.split('?')[1] ?? '')));
    }
  });

  const filt = $derived(apiFilters(f));
  const base = $derived({ from: f.range.from, to: f.range.to, ...filt });
  const span = $derived(f.range.to - f.range.from);
  const days = $derived(Math.max(1, Math.round(span / 86_400_000)));
  const usage = useQuery(() => queries.usage({ ...base, group: f.group }));
  const prev = useQuery(() => queries.usage({ ...filt, from: f.range.from - span, to: f.range.from, group: 'month' }));
  const breakdown = useQuery(() => queries.breakdown({ ...base, by: f.by }));
  const byModel = useQuery(() => queries.breakdown({ ...base, by: 'model' }));
  const byProject = useQuery(() => queries.breakdown({ ...base, by: 'project' }));
  const allModels = useQuery(() => queries.breakdown({ from: f.range.from, to: f.range.to, by: 'model' }));
  const allAgents = useQuery(() => queries.breakdown({ from: f.range.from, to: f.range.to, by: 'agent' }));
  const projects = useQuery(() => queries.projects());
  const heat = useQuery(() => queries.heatmap(base));
  const year = useQuery(() => queries.heatmap(filt));

  const withCurrent = (opts: { value: string; label: string }[], value: string, label: string) =>
    value && !opts.some((o) => o.value === value) ? [...opts, { value, label }] : opts;
  const projectOptions = $derived(
    withCurrent(
      (projects.data ?? []).map((p) => ({ value: String(p.id), label: p.name })),
      f.project,
      projects.data?.find((p) => String(p.id) === f.project)?.name ?? f.project,
    ),
  );
  const modelOptions = $derived(withCurrent((allModels.data?.rows ?? []).map((r) => ({ value: r.key, label: modelLabel(r.key) })), f.model, modelLabel(f.model)));
  const agentOptions = $derived(withCurrent((allAgents.data?.rows ?? []).map((r) => ({ value: r.key, label: rowLabel('agent', r) })), f.agent, f.agent));
  const clientOptions = $derived(
    withCurrent(
      [...new Set((projects.data ?? []).map((p) => p.client).filter((c): c is string => !!c))].sort().map((c) => ({ value: c, label: c })),
      f.client,
      f.client,
    ),
  );
  const active = $derived(
    [
      f.project && (projectOptions.find((o) => o.value === f.project)?.label ?? f.project),
      f.model && modelLabel(f.model),
      f.agent && (f.agent === 'main' ? t('common.main') : f.agent),
      f.client,
    ].filter((x): x is string => !!x),
  );
  const summary = $derived(active.join(', '));

  const total = $derived(usage.data?.total);
  const prevCost = $derived(prev.data?.total.cost_usd);
  const delta = $derived(total && prevCost !== undefined && prevCost > 1e-9 ? ((total.cost_usd - prevCost) / prevCost) * 100 : null);
  const tokens = $derived(total ? total.tokens.input + total.tokens.output + total.tokens.cache_read + total.tokens.cache_write_5m + total.tokens.cache_write_1h : 0);
  const cacheRatio = $derived.by(() => {
    if (!total) return null;
    const d = total.tokens.cache_read + total.tokens.cache_write_5m + total.tokens.cache_write_1h + total.tokens.input;
    return d > 0 ? total.tokens.cache_read / d : null;
  });
  const topModel = $derived(byModel.data?.rows[0]);
  const topProject = $derived(byProject.data?.rows[0]);
  const dates = $derived(rangeText(f.range.from, f.range.to));

  function linkFor(r: BreakdownRow): string | undefined {
    if (!r.key) return undefined;
    if (f.by === 'session') return href(`/sessions/${encodeURIComponent(r.key)}`);
    if (f.by !== 'project' && f.by !== 'model' && f.by !== 'agent' && f.by !== 'client') return undefined;
    if (f[f.by] === r.key) return undefined;
    return href('/usage', toQuery({ ...f, [f.by]: r.key, by: f.by === 'project' ? 'model' : 'project' }));
  }

  function clear() {
    f = { ...f, project: '', model: '', agent: '', client: '' };
  }
  usePageExport(() => ({ kind: 'server', view: 'usage', query: { from: f.range.from, to: f.range.to, group: f.group, ...filt } }));
</script>

<PageHeader title={t('pages.usage.title')} lede={t('pages.usage.lede')}>
  {#snippet meta()}
    <Button size="sm" icon="download" onclick={() => (exportOpen = true)}>{t('pages.usage.export.button')}</Button>
  {/snippet}
  {#snippet filters()}
    <DateRangePicker bind:value={f.range} />
    <Tabs label={t('pages.usage.granularity')} items={(['hour', 'day', 'week', 'month'] as const).map((g) => ({ id: g, label: t(`pages.usage.group.${g}`) }))} bind:value={f.group} />
    <div class="selects">
      <Select label={t('pages.usage.filters.project')} hideLabel bind:value={f.project} options={[{ value: '', label: t('pages.usage.filters.all_projects') }, ...projectOptions]} />
      <Select label={t('pages.usage.filters.model')} hideLabel bind:value={f.model} options={[{ value: '', label: t('pages.usage.filters.all_models') }, ...modelOptions]} />
      <span class="with-help">
        <Select label={t('pages.usage.filters.agent')} hideLabel bind:value={f.agent} options={[{ value: '', label: t('pages.usage.filters.all_agents') }, ...agentOptions]} />
        <InlineHelp text={t('pages.usage.agent_help')} label={t('pages.usage.filters.agent')} />
      </span>
      <Select label={t('pages.usage.filters.client')} hideLabel bind:value={f.client} options={[{ value: '', label: t('pages.usage.filters.all_clients') }, ...clientOptions]} />
      {#if active.length}<Button variant="ghost" icon="x" onclick={clear}>{t('pages.usage.filters.clear')}</Button>{/if}
    </div>
  {/snippet}
</PageHeader>

{#if usage.error && !usage.data}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(usage.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => usage.refetch()}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else if !usage.data || !total}
  <div class="stack" aria-busy="true">
    <Skeleton height="1.3rem" width="min(640px, 100%)" lines={2} />
    <Card><Skeleton height="80px" /></Card>
    <Card><Skeleton height="260px" /></Card>
  </div>
{:else}
  <Answer confidence={usage.data.confidence} refreshing={usage.refreshing}>
    {#if total.requests === 0}
      {t('pages.usage.answer_empty', dates)}
    {:else}
      <Rich text={t('pages.usage.answer_spent', { ...dates, money: strong(fmt.money(total.cost_usd)) })} />
      {#if delta !== null}
        {#if Math.abs(delta) < 2}{t('pages.usage.answer_flat')}{:else}<Rich text={t(delta > 0 ? 'pages.usage.answer_up' : 'pages.usage.answer_down', { pct: strong(fmt.pct(Math.abs(delta))) })} />{/if}
      {/if}
      {#if topModel && !f.model}<Rich text={t('pages.usage.answer_model', { model: strong(modelLabel(topModel.key)), share: fmt.ratio(topModel.share) })} />{/if}
      {#if topProject && !f.project}<Rich text={t('pages.usage.answer_project', { project: strong(topProject.label), money: fmt.money(topProject.usage.cost_usd) })} />{/if}
      {#if total.unpriced_requests}{tn('confidence.unpriced', total.unpriced_requests)}.{/if}
      {#if summary}<span class="filtered">{t('pages.usage.answer_filtered', { filters: summary })}</span>{/if}
    {/if}
  </Answer>

  {#if total.requests === 0}
    <Card>
      <EmptyState icon="usage" title={t('pages.usage.empty_title')} body={active.length ? t('pages.usage.empty_filtered') : t('pages.usage.empty_body')}>
        {#snippet action()}
          {#if active.length}<Button onclick={clear}>{t('pages.usage.filters.clear')}</Button>{/if}
        {/snippet}
      </EmptyState>
    </Card>
  {:else}
    <div class="stack">
      <FigureStrip label={t('pages.usage.title')} refreshing={usage.refreshing}>
        <Stat label={t('pages.usage.stat.cost')} {delta} reference={t('pages.usage.ref_prev')} meta={tn('units.requests', total.requests)}>
          {#snippet value()}<Money usd={total.cost_usd} confidence={usage.data?.confidence ?? 'lower_bound'} unpriced={total.unpriced_requests} size="lg" />{/snippet}
        </Stat>
        <Stat label={t('pages.usage.stat.per_day')} meta={tn('pages.usage.days', days)}>
          {#snippet value()}<Money usd={total.cost_usd / days} confidence={usage.data?.confidence ?? 'lower_bound'} size="lg" badge="seal" />{/snippet}
        </Stat>
        <Figure label={t('pages.usage.stat.tokens')} help={t('pages.usage.stat.tokens_help')} value={fmt.compact(tokens)} meta={t('pages.usage.tokens_split', { read: fmt.compact(total.tokens.cache_read), out: fmt.compact(total.tokens.output) })} />
        <Figure label={t('pages.usage.stat.cache')} help={t('pages.usage.stat.cache_help')} value={cacheRatio === null ? t('common.unknown') : fmt.ratio(cacheRatio)} meta={t('pages.usage.cache_meta')} />
      </FigureStrip>

      <OverTime report={usage.data} group={f.group} {days} refreshing={usage.refreshing} />

      {#each [breakdown, heat, year] as q, i (i)}
        {#if q.error && !q.refreshing}
          <Banner tone="warning" title={t('common.error_title')} body={describeError(q.error)}>
            {#snippet action()}<Button size="sm" icon="refresh" onclick={() => q.refetch()}>{t('common.retry')}</Button>{/snippet}
          </Banner>
        {/if}
      {/each}

      <div class="pair">
        <BreakdownCard data={breakdown.data} bind:by={f.by} {linkFor} refreshing={breakdown.refreshing} />
        <Heatmaps only="hour" heat={heat.data} year={year.data} weekStart={app.settings?.week_start ?? 0} heatRefreshing={heat.refreshing} />
      </div>

      <Compare filtered={active.length > 0} />

      <Heatmaps only="year" heat={heat.data} year={year.data} weekStart={app.settings?.week_start ?? 0} yearRefreshing={year.refreshing} />
    </div>
  {/if}
{/if}

<ExportDialog bind:open={exportOpen} filters={f} {summary} />

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .selects {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
  }
  .pair {
    display: grid;
    grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    gap: var(--sp-5);
    align-items: start;
  }
  .filtered {
    color: var(--ink-3);
  }
  @media (max-width: 1100px) {
    .pair {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .with-help {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
</style>
