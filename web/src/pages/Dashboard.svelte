<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { fmt, i18n, t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/stores/app.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Card from '../lib/ui/Card.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import { describeError } from '../lib/stores/app.svelte';
  import Activity24 from './dashboard/Activity24.svelte';
  import ActiveSessions from './dashboard/ActiveSessions.svelte';
  import BlockCard from './dashboard/BlockCard.svelte';
  import Cost30 from './dashboard/Cost30.svelte';
  import Hero from './dashboard/Hero.svelte';
  import OpenAlerts from './dashboard/OpenAlerts.svelte';
  import Periods from './dashboard/Periods.svelte';
  import StateBanners from './dashboard/StateBanners.svelte';
  import TopCard from './dashboard/TopCard.svelte';

  const overview = useQuery(() => queries.overview());
  const alerts = useQuery(() => queries.alerts({ q: 'open', limit: 4, lang: i18n.lang }));
  const o = $derived(overview.data);
  const hasData = $derived(!!o && o.data.first_request_at !== null);
  const importing = $derived(!!app.ingest?.backfill_running);
  const r = $derived(overview.refreshing);
</script>

<PageHeader title={t('dashboard.title')}>
  {#snippet meta()}
    {#if o}<span>{t('common.updated', { ago: fmt.ago(o.generated_at) })}</span>{/if}
  {/snippet}
</PageHeader>

{#if overview.error && !o}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(overview.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => overview.refetch()}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else if !o}
  <div class="skeleton" aria-busy="true">
    <div class="sk-hero"><Skeleton height="2.2rem" lines={2} /></div>
    <div class="sk-row">
      {#each [0, 1, 2] as i (i)}<div class="sk-card"><Skeleton height="1rem" lines={3} /></div>{/each}
    </div>
    <div class="sk-wide"><Skeleton height="220px" radius="var(--r-md)" /></div>
  </div>
{:else}
  <StateBanners overview={o} ingest={app.ingest} />
  {#if !hasData && !importing}
    <Card>
      <EmptyState icon="terminal" title={t('dashboard.states.no_data_title')} body={t('dashboard.states.no_data_body')} />
    </Card>
  {:else}
    <Hero limits={o.limits} />
    <Periods overview={o} />
    <div class="grid">
      <div class="span-8"><Cost30 daily={o.daily} confidence={o.data.cost_confidence} refreshing={r} /></div>
      <div class="span-4"><BlockCard block={o.active_block} burn={o.burn} confidence={o.data.cost_confidence} refreshing={r} /></div>
      <div class="span-4"><Activity24 hourly={o.hourly} confidence={o.data.cost_confidence} refreshing={r} /></div>
      <div class="span-4"><TopCard projects={o.top_projects} models={o.top_models} confidence={o.data.cost_confidence} refreshing={r} /></div>
      <div class="span-4"><OpenAlerts alerts={alerts.data?.items ?? []} refreshing={alerts.refreshing} /></div>
      <div class="span-12"><ActiveSessions sessions={o.active_sessions} confidence={o.data.cost_confidence} refreshing={r} /></div>
    </div>
  {/if}
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: var(--sp-5);
    margin-top: var(--sp-5);
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
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .sk-hero {
    max-width: 520px;
    padding: var(--sp-4) 0 var(--sp-6);
  }
  .sk-row {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--sp-5);
  }
  .sk-card,
  .sk-wide {
    padding: var(--sp-5);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
  }
  @media (max-width: 1180px) {
    .span-8 {
      grid-column: span 12;
    }
    .span-4 {
      grid-column: span 6;
    }
  }
  @media (max-width: 860px) {
    .grid {
      gap: var(--sp-4);
    }
    .grid > div {
      grid-column: span 12;
    }
    .sk-row {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
