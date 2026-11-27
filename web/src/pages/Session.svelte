<script lang="ts">
  import type { BreakdownRow } from '../lib/api/contract';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { modelColor, modelLabel, modelTitle } from '../lib/charts/series';
  import { fmt, t, tn } from '../lib/i18n/index.svelte';
  import { href } from '../lib/router';
  import { api, describeError } from '../lib/stores/app.svelte';
  import { untitled } from '../lib/session-title';
  import { usePageExport } from '../lib/shell/exports.svelte';
  import { router } from '../lib/stores/router.svelte';
  import Answer from '../lib/ui/Answer.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import Icon from '../lib/ui/Icon.svelte';
  import InlineHelp from '../lib/ui/InlineHelp.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Rich, { strong } from '../lib/ui/Rich.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import Stat from '../lib/ui/Stat.svelte';
  import Cache from './session/Cache.svelte';
  import Context from './session/Context.svelte';
  import Events from './session/Events.svelte';
  import Official from './session/Official.svelte';
  import Split from './session/Split.svelte';
  import Timeline from './session/Timeline.svelte';
  import Transcript from './session/Transcript.svelte';
  import TurnTable from './sessions/TurnTable.svelte';
  import Figure from './usage/Figure.svelte';
  import FigureStrip from './usage/FigureStrip.svelte';

  const id = $derived(router.current.params.id ?? '');
  const detail = useQuery(() => queries.session(id));
  const d = $derived(detail.data);
  const s = $derived(d?.summary);
  const tokens = $derived(s ? s.usage.tokens.input + s.usage.tokens.output + s.usage.tokens.cache_read + s.usage.tokens.cache_write_5m + s.usage.tokens.cache_write_1h : 0);
  const agentShare = $derived(s && s.usage.cost_usd > 0 ? s.subagent_cost_usd / s.usage.cost_usd : 0);
  const agentLabel = (r: BreakdownRow) => (r.key === 'main' ? t('common.main') : r.label);
  const lede = $derived(s ? [s.project, s.git_branch, fmt.dateTime(s.started_at)].filter(Boolean).join(', ') : undefined);
  usePageExport(() => {
    const sid = id;
    return { kind: 'json', name: `session-${sid.slice(0, 8)}`, data: async () => (detail.data?.summary.id === sid ? detail.data : api().session(sid)) };
  });
</script>

<p class="back"><a href={href('/sessions')}><Icon name="chevron_left" size={16} />{t('pages.session.back')}</a></p>
<PageHeader title={s ? (s.title ?? untitled()) : t('pages.session.title')} {lede}>
  {#snippet meta()}
    {#if s}
      {#if s.is_active}<span class="state live"><span class="pulse"></span>{t('pages.session.running')}</span>
      {:else}<span class="state">{t('pages.session.ended', { ago: fmt.ago(s.last_at) })}</span>{/if}
    {/if}
  {/snippet}
</PageHeader>

{#if detail.error && !d}
  {#if detail.error.code === 'not_found'}
    <Card>
      <EmptyState icon="sessions" title={t('pages.session.not_found_title')} body={t('pages.session.not_found_body')}>
        {#snippet action()}<Button href={href('/sessions')}>{t('pages.session.back')}</Button>{/snippet}
      </EmptyState>
    </Card>
  {:else}
    <Banner tone="warning" title={t('common.error_title')} body={describeError(detail.error)}>
      {#snippet action()}<Button icon="refresh" onclick={() => detail.refetch()}>{t('common.retry')}</Button>{/snippet}
    </Banner>
  {/if}
{:else if !d || !s}
  <div class="stack" aria-busy="true">
    <Skeleton height="1.3rem" width="min(560px, 100%)" />
    <Card><Skeleton height="72px" /></Card>
    <Card><Skeleton height="220px" /></Card>
  </div>
{:else}
  <Answer confidence="lower_bound" refreshing={detail.refreshing}>
    <Rich
      text={t(s.is_active ? 'pages.session.answer_running' : 'pages.session.answer_ended', {
        duration: fmt.duration(s.last_at - s.started_at),
        money: strong(fmt.money(s.usage.cost_usd)),
      })}
    />
    {#if agentShare >= 0.01}<Rich text={t('pages.session.answer_agents', { share: strong(fmt.ratio(agentShare)) })} />{/if}
    {#if s.main_model}{t('pages.session.answer_model', { model: modelLabel(s.main_model) })}{/if}
  </Answer>

  <div class="stack">
    <FigureStrip label={t('pages.session.title')} refreshing={detail.refreshing}>
      <Stat label={t('pages.session.stat.cost')} meta={tn('units.requests', s.usage.requests)}>
        {#snippet value()}<Money usd={s.usage.cost_usd} confidence="lower_bound" unpriced={s.usage.unpriced_requests} size="lg" />{/snippet}
      </Stat>
      {#if s.is_active}
        <Stat
          label={t('dashboard.block.burn')}
          meta={d.burn.active
            ? t('dashboard.block.burn_meta', { tokens: fmt.compact(d.burn.tokens_per_min), minutes: fmt.number(d.burn.window_minutes) })
            : t('dashboard.block.idle', { minutes: d.burn.window_minutes })}
        >
          {#snippet value()}<span class="burn"><Money usd={d.burn.usd_per_hour} confidence="estimate" size="lg" badge="none" /><span class="unit">{t('dashboard.block.per_hour')}</span><ConfidenceBadge confidence="estimate" compact /></span>{/snippet}
        </Stat>
      {/if}
      <Figure label={t('pages.session.stat.duration')} value={fmt.duration(s.last_at - s.started_at)} meta={`${fmt.time(s.started_at)} - ${fmt.time(s.last_at)}`} />
      <Figure label={t('pages.session.stat.tokens')} help={t('pages.usage.stat.tokens_help')} value={fmt.compact(tokens)} meta={t('pages.usage.tokens_split', { read: fmt.compact(s.usage.tokens.cache_read), out: fmt.compact(s.usage.tokens.output) })} />
      <Figure label={t('pages.session.stat.cache')} help={t('pages.usage.stat.cache_help')} value={s.cache_hit_ratio === null ? t('common.unknown') : fmt.ratio(s.cache_hit_ratio)} meta={t('pages.usage.cache_meta')} />
    </FigureStrip>

    <div class="grid">
      <div class="span-8"><Timeline timeline={d.timeline} refreshing={detail.refreshing} /></div>
      <div class="span-4"><Official detail={d} /></div>
      <div class="span-4">
        <Split title={t('pages.session.by_model')} nameColumn={t('pages.usage.dims.model')} rows={d.by_model} confidence="lower_bound" label={(r) => modelLabel(r.key)} tooltip={(r) => modelTitle(r.key)} color={(r) => modelColor(r.key)} />
      </div>
      <div class="span-4">
        <Split title={t('pages.session.by_agent')} nameColumn={t('pages.usage.dims.agent')} rows={d.by_agent} confidence="lower_bound" label={agentLabel} help={t('pages.session.agent_help')} />
      </div>
      <div class="span-4">
        <Split title={t('pages.session.tools')} nameColumn={t('pages.usage.dims.tool')} rows={d.tools} confidence="estimate" label={(r) => r.label} help={t('pages.session.tools_help')} />
      </div>
      <div class="span-4"><Context detail={d} /></div>
      <div class="span-8"><Cache detail={d} /></div>
      <div class="span-12">
        <Card title={t('pages.session.top_turns')} flush>
          <TurnTable rows={d.top_turns} caption={t('pages.session.top_turns')} linkSessions={false} />
        </Card>
      </div>
      <div class="span-4"><Events events={d.events} /></div>
      <div class="span-8"><Transcript id={s.id} /></div>
    </div>
  </div>
{/if}

<style>
  .burn {
    display: inline-flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 4px;
  }
  .unit {
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-regular);
  }
  .back {
    margin-bottom: var(--sp-3);
    font-size: var(--fs-sm);
  }
  .back a {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    text-decoration: none;
  }
  .back a:hover {
    text-decoration: underline;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 2px var(--sp-3);
    border-radius: var(--r-pill);
    background: var(--surface-3);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .state.live {
    background: var(--good-soft);
    color: var(--good-ink);
    font-weight: var(--fw-medium);
  }
  .pulse {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--good);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: var(--sp-5);
  }
  .grid > div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .grid > div > :global(.card) {
    flex: 1;
  }
  .span-12 {
    grid-column: span 12;
  }
  .span-8 {
    grid-column: span 8;
  }
  .span-4 {
    grid-column: span 4;
  }
  @media (max-width: 1180px) {
    .span-8,
    .span-4 {
      grid-column: span 6;
    }
    .span-8:has(+ .span-4) {
      grid-column: span 12;
    }
  }
  @media (max-width: 860px) {
    .grid {
      gap: var(--sp-4);
    }
    .grid > div {
      grid-column: span 12;
    }
  }
</style>
