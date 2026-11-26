<script lang="ts">
  import type { BackupInfo } from '../../lib/api/contract';
  import { queries } from '../../lib/api/queries';
  import { useQuery } from '../../lib/api/query.svelte';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { api, cache, describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';
  import { bytes } from './util';

  let { dataDir }: { dataDir: string } = $props();
  const list = useQuery(() => queries.backups());
  let busy = $state(false);
  let fresh = $state<string | null>(null);
  let sortKey = $state('when');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const isDb = (b: BackupInfo) => b.file.endsWith('.db');

  const columns = $derived<Column<BackupInfo>[]>([
    { key: 'file', label: t('system.backups.file'), value: (b) => b.file },
    { key: 'when', label: t('system.backups.when'), value: (b) => b.created_at },
    { key: 'size', label: t('system.backups.size'), align: 'end', optional: true, value: (b) => b.size_bytes },
  ]);

  async function create() {
    busy = true;
    try {
      const b = await api().createBackup();
      fresh = b.file;
      cache.invalidate(['system']);
      toasts.push('success', t('system.backups.created', { file: b.file }));
    } catch (e) {
      toasts.push('error', describeError(e));
    } finally {
      busy = false;
    }
  }
</script>

<Card id="backups" title={t('system.backups.title')} subtitle={t('system.backups.subtitle', { dir: `${dataDir.replace(/\/$/, '')}/backups` })} refreshing={list.refreshing} flush>
  <div class="bar"><Button icon="download" {busy} onclick={create}>{t('system.backups.create')}</Button></div>
  {#if list.data && list.data.length}
    <DataTable caption={t('system.backups.title')} rows={list.data} {columns} rowKey={(b) => b.file + b.created_at} bind:sortKey bind:sortDir dense>
      {#snippet cell(b, col)}
        {#if col.key === 'file'}
          <span class="file" class:fresh={fresh === b.file}><Icon name={isDb(b) ? 'system' : 'settings'} size={15} /><code>{b.file}</code></span>
          <span class="kind">{isDb(b) ? t('system.backups.kind_db') : t('system.backups.kind_settings')}</span>
        {:else if col.key === 'when'}{fmt.dateTime(b.created_at)}
        {:else}<span class="num">{bytes(b.size_bytes)}</span>{/if}
      {/snippet}
    </DataTable>
  {:else if list.data}
    <div class="pad"><EmptyState compact icon="download" title={t('system.backups.empty')} /></div>
  {:else if list.error}
    <p class="pad err">{describeError(list.error)}</p>
  {:else}
    <div class="pad"><Skeleton lines={3} height="1.2em" /></div>
  {/if}
  {#snippet footer()}<p class="restore"><Icon name="terminal" size={15} />{t('system.backups.restore')}</p>{/snippet}
</Card>

<style>
  .file {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    font-weight: var(--fw-medium);
    overflow-wrap: anywhere;
  }
  .file :global(svg) {
    flex: none;
    color: var(--ink-3);
  }
  .file.fresh code {
    background: var(--good-soft);
    color: var(--good-ink);
    border-radius: var(--r-xs);
    padding: 0 4px;
  }
  .kind {
    display: block;
    margin-left: calc(15px + var(--sp-2));
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .bar {
    padding: 0 var(--sp-5) var(--sp-4);
  }
  .pad {
    padding: 0 var(--sp-5) var(--sp-5);
  }
  .err {
    color: var(--critical-ink);
  }
  .restore {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .restore :global(svg) {
    flex: none;
    color: var(--ink-3);
  }
</style>
