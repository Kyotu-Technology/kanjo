<script lang="ts">
  import type { IngestStatus, Overview } from '../../lib/api/contract';
  import { modelLabel } from '../../lib/charts/series';
  import { fmt, t, tn } from '../../lib/i18n/index.svelte';
  import ConnectPanel from '../../lib/shell/ConnectPanel.svelte';
  import Banner from '../../lib/ui/Banner.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';

  interface Props {
    overview: Overview;
    ingest: IngestStatus | null;
  }
  let { overview: o, ingest }: Props = $props();
  let connectOpen = $state(false);
  const importing = $derived(!!ingest?.backfill_running);
  const hasData = $derived(o.data.first_request_at !== null);
</script>

<div class="banners">
  {#if importing && ingest}
    <Banner
      tone="progress"
      title={t('dashboard.states.import_title')}
      body={t('dashboard.states.import_body', { done: fmt.number(ingest.files_done), total: fmt.number(ingest.files_total), requests: fmt.number(ingest.requests_stored) })}
      progress={ingest.files_total ? ingest.files_done / ingest.files_total : 0}
    />
  {/if}
  {#if hasData && !o.data.statusline_connected}
    <Banner icon="plug" title={t('dashboard.states.no_statusline_title')} body={t('dashboard.states.no_statusline_body')}>
      {#snippet action()}
        <Button variant="primary" icon="plug" onclick={() => (connectOpen = true)}>{t('dashboard.states.no_statusline_action')}</Button>
      {/snippet}
    </Banner>
  {/if}
  {#if o.data.unpriced_models.length}
    <Banner
      tone="warning"
      title={t('dashboard.states.unpriced_title')}
      body={t('dashboard.states.unpriced_body', { models: o.data.unpriced_models.map(modelLabel).join(', ') }) +
        (o.data.quarantined_records > 0 ? ' ' + tn('dashboard.states.quarantine', o.data.quarantined_records) : '')}
    />
  {/if}
</div>

<Dialog bind:open={connectOpen} title={t('onboarding.connect.title')} description={t('onboarding.connect.body')} size="md">
  <ConnectPanel onconnected={() => setTimeout(() => (connectOpen = false), 1200)} />
</Dialog>

<style>
  .banners {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  .banners:not(:empty) {
    margin-bottom: var(--sp-6);
  }
</style>
