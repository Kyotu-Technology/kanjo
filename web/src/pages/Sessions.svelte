<script lang="ts">
  import { untrack } from 'svelte';
  import type { SessionSummary } from '../lib/api/contract';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { modelColor, modelLabel, modelTitle } from '../lib/charts/series';
  import { fmt, i18n, t, tn } from '../lib/i18n/index.svelte';
  import { PRESETS, customRange, rangeFor, toDateInput, type DateRange } from '../lib/range';
  import { href } from '../lib/router';
  import { describeError } from '../lib/stores/app.svelte';
  import { runExport } from '../lib/shell/actions';
  import { usePageExport, type PageExport } from '../lib/shell/exports.svelte';
  import { navigate, router } from '../lib/stores/router.svelte';
  import Answer from '../lib/ui/Answer.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../lib/ui/DataTable.svelte';
  import DateRangePicker from '../lib/ui/DateRangePicker.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import InlineHelp from '../lib/ui/InlineHelp.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Rich, { strong } from '../lib/ui/Rich.svelte';
  import Select from '../lib/ui/Select.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import Tabs from '../lib/ui/Tabs.svelte';
  import Turns from './sessions/Turns.svelte';
  import { titlesOff, untitled } from '../lib/session-title';

  const PAGE = 25;
  const SORTS: Record<string, string> = { cost: 'cost', last: 'recent', tokens: 'tokens', duration: 'duration' };
  const q0 = router.current.query;
  const preset0 = (PRESETS as readonly string[]).includes(q0.range ?? '') ? (q0.range as (typeof PRESETS)[number]) : 'last_90';

  let tab = $state<'sessions' | 'turns'>(q0.tab === 'turns' ? 'turns' : 'sessions');
  let search = $state(q0.q ?? '');
  let debounced = $state(q0.q ?? '');
  let project = $state(q0.project ?? '');
  let range = $state<DateRange>((q0.from && q0.to ? customRange(q0.from, q0.to) : null) ?? rangeFor(preset0, i18n.now));
  let sortKey = $state(Object.keys(SORTS).includes(q0.sort ?? '') ? (q0.sort as string) : 'cost');
  let sortDir = $state<'asc' | 'desc'>('desc');
  let page = $state(Math.max(0, Number(q0.page ?? 1) - 1) || 0);

  $effect(() => {
    const v = search;
    const id = setTimeout(() => (debounced = v.trim()), 250);
    return () => clearTimeout(id);
  });
  $effect(() => {
    if (sortDir !== 'desc') sortDir = 'desc';
  });
  $effect(() => {
    const p = router.current.query.project ?? '';
    untrack(() => {
      if (p !== project) project = p;
    });
  });
  let filterKey = '';
  $effect(() => {
    const key = JSON.stringify([debounced, project, range.from, range.to, sortKey]);
    if (filterKey && key !== filterKey) page = 0;
    filterKey = key;
  });
  $effect(() => {
    const target = href('/sessions', {
      tab: tab === 'turns' ? 'turns' : undefined,
      q: debounced || undefined,
      project: project || undefined,
      range: range.preset !== 'custom' && range.preset !== 'last_90' ? range.preset : undefined,
      from: range.preset === 'custom' ? toDateInput(range.from) : undefined,
      to: range.preset === 'custom' ? toDateInput(range.to - 1) : undefined,
      sort: sortKey !== 'cost' ? sortKey : undefined,
      page: page > 0 ? page + 1 : undefined,
    });
    if (location.hash !== target) history.replaceState(history.state, '', target);
  });

  const pid = $derived(project ? Number(project) : undefined);
  const scope = $derived({ from: range.from, to: range.to, project: pid });
  const list = useQuery(() => queries.sessions({ ...scope, q: debounced || undefined, sort: SORTS[sortKey], limit: PAGE, offset: page * PAGE }));
  const top = useQuery(() => queries.sessions({ ...scope, q: debounced || undefined, sort: 'cost', limit: 1 }));
  const spent = useQuery(() => queries.usage({ ...scope, group: 'month' }));
  const overview = useQuery(() => queries.overview());
  const projects = useQuery(() => queries.projects());
  const runningNow = $derived((overview.data?.active_sessions ?? []).filter((s) => pid === undefined || s.project_id === pid));
  const running = $derived(runningNow.length);
  const total = $derived(list.data?.total ?? 0);
  const pages = $derived(Math.max(1, Math.ceil(total / PAGE)));
  const projectOptions = $derived([
    { value: '', label: t('pages.sessions.all_projects') },
    ...(projects.data ?? []).map((p) => ({ value: String(p.id), label: p.name })),
    ...(project && !projects.data?.some((p) => String(p.id) === project) ? [{ value: project, label: project }] : []),
  ]);
  const tokens = (s: SessionSummary) => s.usage.tokens.input + s.usage.tokens.output + s.usage.tokens.cache_read + s.usage.tokens.cache_write_5m + s.usage.tokens.cache_write_1h;
  const columns = $derived<Column<SessionSummary>[]>([
    { key: 'session', label: t('dashboard.sessions.session') },
    { key: 'model', label: t('dashboard.sessions.model'), optional: true },
    { key: 'last', label: t('pages.sessions.last'), optional: true, value: (s) => s.last_at },
    { key: 'duration', label: t('pages.sessions.duration'), align: 'end', optional: true, value: (s) => s.last_at - s.started_at },
    { key: 'tokens', label: t('pages.sessions.tokens'), align: 'end', optional: true, value: tokens },
    { key: 'cache', label: t('pages.sessions.cache'), align: 'end', optional: true, header: cacheHelp },
    { key: 'cost', label: t('dashboard.sessions.cost'), align: 'end', value: (s) => s.usage.cost_usd, header: badge },
  ]);
  const sortLabel = $derived(columns.find((c) => c.key === sortKey)?.label ?? '');
  const open = (s: SessionSummary) => href(`/sessions/${encodeURIComponent(s.id)}`);

  function clearFilters() {
    search = '';
    debounced = '';
    project = '';
    range = rangeFor('last_90', i18n.now);
  }
  const exp = $derived<PageExport>(
    tab === 'turns'
      ? { kind: 'server', view: 'turns', query: { ...scope } }
      : { kind: 'server', view: 'sessions', query: { ...scope, q: debounced || undefined, sort: SORTS[sortKey] } },
  );
  usePageExport(() => exp);
</script>

{#snippet badge()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}
{#snippet cacheHelp()}<InlineHelp text={t('pages.sessions.cache_help')} label={t('pages.sessions.cache')} />{/snippet}

<PageHeader title={t('pages.sessions.title')} lede={t('pages.sessions.lede')}>
  {#snippet meta()}
    <Button size="sm" icon="download" onclick={() => runExport(exp)}>{t('common.export')}</Button>
  {/snippet}
  {#snippet filters()}
    <label class="search">
      <span class="sr-only">{t('pages.sessions.search')}</span>
      <input type="search" bind:value={search} placeholder={t('pages.sessions.search')} />
    </label>
    <Select label={t('pages.sessions.project')} hideLabel bind:value={project} options={projectOptions} />
    <DateRangePicker bind:value={range} />
  {/snippet}
</PageHeader>

{#if titlesOff()}
  <div class="banner">
    <Banner icon="lock" title={t('pages.sessions.titles_off_title')} body={t('pages.sessions.titles_off_body')}>
      {#snippet action()}<Button href={href('/settings', { section: 'privacy' })} size="sm">{t('pages.sessions.titles_off_action')}</Button>{/snippet}
    </Banner>
  </div>
{/if}

<div class="tabs">
  <Tabs
    label={t('pages.sessions.tabs')}
    items={[
      { id: 'sessions', label: t('pages.sessions.tab_sessions') },
      { id: 'turns', label: t('pages.sessions.tab_turns') },
    ]}
    bind:value={tab}
  />
</div>

{#if tab === 'turns'}
  <Turns from={range.from} to={range.to} project={pid} />
{:else if list.error && !list.data}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(list.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => list.refetch()}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else if !list.data}
  <div aria-busy="true">
    <Skeleton height="1.3rem" width="min(560px, 100%)" />
    <div class="gap"></div>
    <Card><Skeleton lines={8} height="1.4em" /></Card>
  </div>
{:else if !total}
  <Card>
    {#if debounced}
      <EmptyState icon="search" title={t('pages.sessions.empty_search_title', { query: debounced })} body={t('pages.sessions.empty_search_body')}>
        {#snippet action()}<Button onclick={clearFilters}>{t('pages.sessions.clear')}</Button>{/snippet}
      </EmptyState>
    {:else}
      <EmptyState icon="sessions" title={t('pages.sessions.empty_title')} body={t('pages.sessions.empty_body')}>
        {#snippet action()}{#if project || range.preset !== 'last_90'}<Button onclick={clearFilters}>{t('pages.sessions.clear')}</Button>{/if}{/snippet}
      </EmptyState>
    {/if}
  </Card>
{:else}
  <Answer confidence="lower_bound" refreshing={list.refreshing}>
    {#if debounced}
      {tn('pages.sessions.answer_search', total, { query: debounced })}
    {:else}
      <Rich text={tn('pages.sessions.answer', total, { money: strong(fmt.money(spent.data?.total.cost_usd ?? 0)) })} />
    {/if}
    {#if running}{tn('pages.sessions.answer_running', running)}{/if}
    {#if top.data?.items[0]}
      {@const s = top.data.items[0]}
      <Rich text={t('pages.sessions.answer_top', { title: s.title ?? s.project, money: strong(fmt.money(s.usage.cost_usd)) })} />
    {/if}
  </Answer>

  {#if runningNow.length && page === 0 && !debounced}
    <ul class="running" aria-label={t('pages.sessions.running_now')}>
      {#each runningNow as r (r.id)}
        <li>
          <a href={open(r)}>
            <span class="pulse"></span>
            <span class="rt">{r.title ?? untitled()}</span>
            <span class="rp">{r.project}</span>
            <Money usd={r.usage.cost_usd} confidence="lower_bound" size="sm" badge="seal" />
          </a>
        </li>
      {/each}
    </ul>
  {/if}

  <Card flush refreshing={list.refreshing}>
    <DataTable caption={t('pages.sessions.title')} rows={list.data.items} {columns} rowKey={(s) => s.id} bind:sortKey bind:sortDir onrowclick={(s) => navigate(open(s))}>
      {#snippet cell(s, col)}
        {#if col.key === 'session'}
          <span class="first">
            {#if s.is_active}<span class="live" title={t('pages.sessions.active_help')}><span class="pulse"></span><span class="sr-only">{t('pages.sessions.active')}</span></span>{/if}
            <span>
              <a class="title" href={open(s)} onclick={(e) => e.stopPropagation()}>{s.title ?? untitled()}</a>
              <span class="sub">
                {#if s.is_active}<span class="run">{t('pages.sessions.active')}</span>{/if}{s.project}{#if s.git_branch}, {s.git_branch}{/if}
              </span>
            </span>
          </span>
        {:else if col.key === 'model'}
          {#if s.main_model}<span class="model" title={modelTitle(s.main_model)}><span class="key" style="background:{modelColor(s.main_model)}"></span>{modelLabel(s.main_model)}</span>{/if}
          {#if s.models.length > 1}<span class="sub" title={s.models.filter((m) => m !== s.main_model).join(', ')}>{s.models.filter((m) => m !== s.main_model).map(modelLabel).join(', ')}</span>{/if}
        {:else if col.key === 'last'}{fmt.ago(s.last_at)}<span class="sub">{fmt.dateTime(s.last_at)}</span>
        {:else if col.key === 'duration'}{fmt.duration(s.last_at - s.started_at)}
        {:else if col.key === 'tokens'}{fmt.compact(tokens(s))}
        {:else if col.key === 'cache'}{s.cache_hit_ratio === null ? '' : fmt.ratio(s.cache_hit_ratio)}
        {:else}<Money usd={s.usage.cost_usd} confidence="lower_bound" unpriced={s.usage.unpriced_requests} size="sm" badge="none" />{/if}
      {/snippet}
    </DataTable>
    <nav class="pager" aria-label={t('pages.sessions.pages')}>
      <span class="sorted">{t('pages.sessions.sorted', { column: sortLabel })}</span>
      <span class="pos">{t('pages.sessions.page', { from: fmt.number(page * PAGE + 1), to: fmt.number(Math.min(total, (page + 1) * PAGE)), total: fmt.number(total) })}</span>
      <Button size="sm" icon="chevron_left" iconOnly disabled={page === 0} onclick={() => (page = Math.max(0, page - 1))}>{t('pages.sessions.prev')}</Button>
      <Button size="sm" icon="chevron_right" iconOnly disabled={page >= pages - 1} onclick={() => (page = Math.min(pages - 1, page + 1))}>{t('pages.sessions.next')}</Button>
    </nav>
  </Card>
{/if}

<style>
  .search input {
    min-height: 36px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    font-size: var(--fs-sm);
  }
  .search input {
    width: 100%;
  }
  .search {
    flex: 0 1 340px;
    min-width: min(240px, 100%);
  }
  .tabs {
    margin: calc(-1 * var(--sp-2)) 0 var(--sp-5);
  }
  .banner {
    margin-bottom: var(--sp-5);
  }
  .gap {
    height: var(--sp-5);
  }
  .first {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
  }
  .live {
    display: inline-flex;
    width: 10px;
    height: 10px;
    margin-top: 5px;
    flex: none;
    align-items: center;
    justify-content: center;
  }
  .pulse {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--good);
    box-shadow: 0 0 0 3px var(--good-soft);
  }
  .run {
    color: var(--good-ink);
    font-weight: var(--fw-medium);
    margin-right: 6px;
  }
  .title {
    color: var(--ink);
    font-weight: var(--fw-semibold);
    text-decoration: none;
  }
  .title:hover {
    text-decoration: underline;
  }
  .sub {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .model {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  .key {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .running {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
    margin: calc(-1 * var(--sp-3)) 0 var(--sp-4);
  }
  .running a {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 36px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--line);
    border-radius: var(--r-pill);
    background: var(--surface);
    color: var(--ink);
    font-size: var(--fs-sm);
    text-decoration: none;
  }
  .running a:hover {
    border-color: var(--line-strong);
  }
  .rt {
    font-weight: var(--fw-semibold);
    max-width: 30ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rp {
    color: var(--ink-3);
  }
  .pager {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--sp-2);
    padding: var(--sp-3) var(--sp-4);
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .sorted {
    margin-right: auto;
    font-size: var(--fs-xs);
  }
  @media (max-width: 600px) {
    .rp,
    .sorted {
      display: none;
    }
    .rt {
      max-width: 22ch;
    }
    .pager {
      justify-content: flex-end;
    }
  }
  .pos {
    font-variant-numeric: tabular-nums;
    margin-right: var(--sp-2);
  }
</style>
