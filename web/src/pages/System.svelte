<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { i18n, t } from '../lib/i18n/index.svelte';
  import { describeError } from '../lib/stores/app.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import '../pages/settings/form.css';
  import SectionNav from './settings/SectionNav.svelte';
  import Audit from './system/Audit.svelte';
  import Backups from './system/Backups.svelte';
  import Checks from './system/Checks.svelte';
  import Connection from './system/Connection.svelte';
  import DataCard from './system/DataCard.svelte';
  import Ingest from './system/Ingest.svelte';
  import Purge from './system/Purge.svelte';
  import Quarantine from './system/Quarantine.svelte';
  import Verdict from './system/Verdict.svelte';

  const system = useQuery(() => queries.system(i18n.lang));
  const chain = useQuery(() => queries.audit({ limit: 1 }));
  const d = $derived(system.data);
  const r = $derived(system.refreshing);
  const issues = $derived(d ? d.checks.filter((c) => c.status !== 'ok').length : 0);
  const quarantined = $derived(d ? d.quarantine.reduce((n, q) => n + q.count, 0) : 0);
  const sections = $derived([
    { id: 'checks', label: t('system.checks.title'), badge: issues ? String(issues) : undefined },
    { id: 'connection', label: t('system.connect.title') },
    { id: 'data', label: t('system.data.title') },
    { id: 'import', label: t('system.ingest.title') },
    { id: 'quarantine', label: t('system.quarantine.title'), badge: quarantined ? String(quarantined) : undefined },
    { id: 'backups', label: t('system.backups.title') },
    { id: 'purge', label: t('system.purge.title') },
    { id: 'audit', label: t('system.audit.title') },
  ]);
</script>

<PageHeader title={t('system.title')} lede={t('system.lede')} />

{#if system.error && !d}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(system.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => system.refetch()}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else if !d}
  <div class="loading" aria-busy="true">
    <div class="sk"><Skeleton height="1.6rem" width="50%" /><Skeleton lines={2} /></div>
    <div class="sk"><Skeleton lines={8} height="1.2em" /></div>
  </div>
{:else}
  <Verdict {d} chainOk={chain.data?.chain_ok ?? true} />
  <div class="layout kj-form">
    <aside class="side"><SectionNav label={t('system.sections')} items={sections} /></aside>
    <div class="stack">
      <Checks checks={d.checks} refreshing={r} />
      <Connection {d} refreshing={r} />
      <DataCard {d} refreshing={r} />
      <Ingest ingest={d.ingest} refreshing={r} />
      <Quarantine rows={d.quarantine} refreshing={r} />
      <Backups dataDir={d.data_dir} />
      <Purge />
      <Audit />
    </div>
  </div>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    gap: var(--sp-8);
    align-items: start;
    margin-top: var(--sp-6);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
    min-width: 0;
  }
  .stack > :global(.card) {
    scroll-margin-top: calc(var(--topbar-h) + var(--sp-4));
  }
  .loading {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    padding: var(--sp-5);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
  }
  @media (max-width: 1180px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    .side {
      display: none;
    }
  }
</style>
