<script lang="ts">
  import type { IngestStatus } from '../../lib/api/contract';
  import { fmt, t, tn } from '../../lib/i18n/index.svelte';
  import { api, app, cache, describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import { bytes } from './util';

  let { ingest, refreshing = false }: { ingest: IngestStatus; refreshing?: boolean } = $props();
  let busy = $state(false);
  let started = $state(false);
  const s = $derived(app.ingest && app.ingest.files_total === ingest.files_total ? app.ingest : ingest);
  const ratio = $derived(s.files_total ? s.files_done / s.files_total : 0);

  $effect(() => {
    if (started && !s.backfill_running) {
      started = false;
      toasts.push('success', t('system.ingest.rescan_done'));
    }
  });

  async function rescan() {
    busy = true;
    try {
      app.ingest = await api().rescan();
      started = app.ingest.backfill_running;
      if (started) toasts.push('info', t('system.ingest.rescan_started'));
      cache.invalidate(['system']);
    } catch (e) {
      toasts.push('error', describeError(e));
    } finally {
      busy = false;
    }
  }
</script>

<Card id="import" title={t('system.ingest.title')} subtitle={t('system.ingest.subtitle')} {refreshing}>
  <p class="watch" class:off={!s.watcher_running}>
    <Icon name={s.watcher_running ? 'dot' : 'warning'} size={16} strokeWidth={2.4} />
    {s.watcher_running ? t('system.ingest.watching') : t('system.ingest.not_watching')}
  </p>
  {#if s.backfill_running}
    <div class="progress" role="status">
      <p class="label">{t('system.ingest.running')}</p>
      <div class="bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(ratio * 100)} aria-label={t('system.ingest.running')}>
        <span style:width="{Math.max(2, ratio * 100)}%"></span>
      </div>
      <p class="hint">{t('system.ingest.progress', { done: fmt.number(s.files_done), total: fmt.number(s.files_total), bytes: bytes(s.bytes_done) })}</p>
    </div>
  {/if}
  <dl class="stats">
    <div><dt>{t('system.ingest.files')}</dt><dd class="num">{fmt.number(s.files_done)} / {fmt.number(s.files_total)}</dd></div>
    <div><dt>{t('system.ingest.requests')}</dt><dd class="num">{tn('units.requests', s.requests_stored)}</dd></div>
    <div><dt>{t('system.ingest.last')}</dt><dd>{s.last_event_at ? fmt.ago(s.last_event_at) : t('system.ingest.never')}</dd></div>
  </dl>
  <div class="rescan">
    <Button icon="refresh" busy={busy} disabled={busy || s.backfill_running} onclick={rescan}>{t('system.ingest.rescan')}</Button>
    <p class="hint">{t('system.ingest.rescan_help')}</p>
  </div>
</Card>

<style>
  .watch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--good-ink);
    font-weight: var(--fw-medium);
  }
  .watch.off {
    color: var(--warning-ink);
  }
  .progress {
    margin-top: var(--sp-4);
  }
  .label {
    font-weight: var(--fw-semibold);
    font-size: var(--fs-sm);
  }
  .bar {
    height: 8px;
    margin: var(--sp-2) 0;
    border-radius: var(--r-pill);
    background: var(--meter-track);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width var(--dur) var(--ease);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--sp-4);
    margin-top: var(--sp-4);
  }
  dt {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  dd {
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
  }
  .rescan {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--sp-3) var(--sp-5);
    margin-top: var(--sp-5);
    padding-top: var(--sp-4);
    border-top: 1px solid var(--line);
  }
  .rescan .hint {
    flex: 1 1 320px;
  }
  @media (max-width: 560px) {
    .stats {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-2);
    }
  }
</style>
