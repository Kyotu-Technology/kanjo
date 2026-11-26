<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { fmt, t } from '../lib/i18n/index.svelte';
  import ConnectPanel from '../lib/shell/ConnectPanel.svelte';
  import { runExport } from '../lib/shell/actions';
  import { usePageExport, type PageExport } from '../lib/shell/exports.svelte';
  import { api } from '../lib/stores/app.svelte';
  import { describeError } from '../lib/stores/app.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Card from '../lib/ui/Card.svelte';
  import Dialog from '../lib/ui/Dialog.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import Answer from './limits/Answer.svelte';
  import BlocksCard from './limits/BlocksCard.svelte';
  import DailyBudget from './limits/DailyBudget.svelte';
  import EstimateCard from './limits/EstimateCard.svelte';
  import HistoryCard from './limits/HistoryCard.svelte';
  import Meters from './limits/Meters.svelte';
  import PerPctCard from './limits/PerPctCard.svelte';

  const limits = useQuery(() => queries.limits());
  const d = $derived(limits.data);
  const cur = $derived(d?.current);
  const official = $derived(!!cur && (!!cur.five_hour || !!cur.seven_day));
  const metered = $derived(official || !!cur?.spend);
  const r = $derived(limits.refreshing);
  let connectOpen = $state(false);
  const exp: PageExport = { kind: 'json', name: 'limits', data: async () => limits.data ?? api().limits() };
  usePageExport(() => exp);
</script>

<PageHeader title={t('pages.limits.title')} lede={t('pages.limits.lede')}>
  {#snippet meta()}
    {#if d}<span>{t('common.updated', { ago: fmt.ago(d.current.five_hour?.observed_at ?? d.current.seven_day?.observed_at ?? Date.now()) })}</span>{/if}
    <Button size="sm" icon="download" onclick={() => runExport(exp)}>{t('common.export_json')}</Button>
  {/snippet}
</PageHeader>

{#snippet connectAction()}
  <Button variant="primary" icon="plug" onclick={() => (connectOpen = true)}>{t('pages.limits.estimate.connect_action')}</Button>
{/snippet}

{#if limits.error && !d}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(limits.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => limits.refetch()}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else if !d || !cur}
  <div class="stack" aria-busy="true">
    <div class="sk-answer"><Skeleton height="2.2rem" lines={2} /></div>
    <div class="grid">
      <div class="span-7"><Card><Skeleton height="1rem" lines={4} /></Card></div>
      <div class="span-5"><Card><Skeleton height="1rem" lines={4} /></Card></div>
      <div class="span-12"><Card><Skeleton height="220px" radius="var(--r-md)" /></Card></div>
    </div>
  </div>
{:else if !metered && !cur.estimate && !d.blocks.length}
  <Card>
    <EmptyState icon="limits" title={t('pages.limits.none_title')} body={t('pages.limits.none_body')} action={connectAction} />
  </Card>
{:else}
  {#if official}
    <Answer limits={cur} />
  {:else if !cur.spend}
    <div class="banner">
      <Banner icon="plug" title={t('pages.limits.estimate.connect_title')} body={t('pages.limits.estimate.connect_body')} action={connectAction} />
    </div>
  {/if}
  <div class="grid">
    {#if metered}
      <div class={cur.seven_day && !cur.spend ? 'span-7' : 'span-12'}><Meters limits={cur} refreshing={r} /></div>
      {#if cur.seven_day}<div class={cur.spend ? 'span-12' : 'span-5'}><DailyBudget week={cur.seven_day} refreshing={r} /></div>{/if}
    {:else if cur.estimate}
      <div class="span-12"><EstimateCard estimate={cur.estimate} active={d.blocks.some((b) => b.is_active)} refreshing={r} /></div>
    {/if}
    <div class="span-12"><BlocksCard blocks={d.blocks} refreshing={r} /></div>
    <div class="span-12"><HistoryCard h5={d.history_5h} h7={d.history_7d} refreshing={r} /></div>
    <div class="span-12"><PerPctCard weeks={d.usd_per_pct_weekly} refreshing={r} /></div>
  </div>
{/if}

<Dialog bind:open={connectOpen} title={t('onboarding.connect.title')} description={t('onboarding.connect.body')} size="md">
  <ConnectPanel onconnected={() => setTimeout(() => ((connectOpen = false), limits.refetch()), 1200)} />
</Dialog>

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .sk-answer {
    max-width: 560px;
    padding-bottom: var(--sp-4);
  }
  .banner {
    margin-bottom: var(--sp-5);
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
  .span-7 {
    grid-column: span 7;
  }
  .span-5 {
    grid-column: span 5;
  }
  @media (max-width: 1180px) {
    .span-7,
    .span-5 {
      grid-column: span 12;
    }
  }
  @media (max-width: 860px) {
    .grid {
      gap: var(--sp-4);
    }
  }
</style>
