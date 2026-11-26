<script lang="ts">
  import type { Alert } from '../../lib/api/contract';
  import { fmt, t, tn } from '../../lib/i18n/index.svelte';
  import { api, app, cache, describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Severity from '../../lib/ui/Severity.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';
  import Tabs from '../../lib/ui/Tabs.svelte';
  import { alertLink } from './links';

  interface Props {
    items: Alert[] | null;
    total: number;
    unread: number;
    filter: 'open' | 'all';
    refreshing?: boolean;
    onbudget: (id: number) => void;
  }
  let { items, total, unread, filter = $bindable(), refreshing = false, onbudget }: Props = $props();
  let busy = $state<number | 'all' | null>(null);

  async function ack(id: number | null) {
    busy = id ?? 'all';
    try {
      const { acknowledged } = await api().ackAlert(id);
      app.unread = Math.max(0, app.unread - acknowledged);
      cache.invalidate(['alerts']);
      toasts.push('success', tn('dashboard.alerts.acked', acknowledged));
    } catch (e) {
      toasts.push('error', describeError(e));
    } finally {
      busy = null;
    }
  }
</script>

<Card title={t('pages.alerts.alerts')} {refreshing}>
  {#snippet actions()}
    <Tabs label={t('pages.alerts.filter')} size="sm" items={[{ id: 'open', label: t('pages.alerts.filter_open') }, { id: 'all', label: t('pages.alerts.filter_all') }]} bind:value={filter} />
  {/snippet}
  {#if items === null}
    <Skeleton lines={6} />
  {:else if items.length}
    <div class="bar">
      <span class="count">{t('pages.alerts.shown', { shown: fmt.number(items.length), total: fmt.number(total) })}</span>
      {#if unread > 1}<Button size="sm" variant="ghost" icon="check" busy={busy === 'all'} disabled={busy !== null} onclick={() => ack(null)}>{t('dashboard.alerts.ack_all')}</Button>{/if}
    </div>
    <ul class="list">
      {#each items as a (a.id)}
        {@const link = alertLink(a)}
        <li class:read={a.acknowledged_at !== null}>
          <div class="sev"><Severity severity={a.severity} showLabel /></div>
          <div class="text">
            <p class="title">{a.title}</p>
            <p class="msg">{a.message}</p>
            <p class="meta">
              <time datetime={new Date(a.ts).toISOString()} title={fmt.dateTime(a.ts)}>{fmt.ago(a.ts)}</time>
              {#if a.acknowledged_at !== null}<span class="readat">{t('pages.alerts.read_at', { ago: fmt.ago(a.acknowledged_at) })}</span>{/if}
            </p>
          </div>
          <div class="act">
            {#if link}
              {#if link.kind === 'budget'}
                <Button size="sm" onclick={() => onbudget(link.id)}>{t('pages.alerts.link.budget')}</Button>
              {:else}
                <Button size="sm" href={link.href}>{t(`pages.alerts.link.${link.kind}`)}</Button>
              {/if}
            {/if}
            {#if a.acknowledged_at === null}
              <Button size="sm" variant="ghost" busy={busy === a.id} disabled={busy !== null} onclick={() => ack(a.id)}>{t('dashboard.alerts.ack')}</Button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {:else if filter === 'open'}
    <EmptyState icon="check" title={t('pages.alerts.open_empty')} body={t('pages.alerts.open_empty_body')}>
      {#snippet action()}<Button size="sm" onclick={() => (filter = 'all')}>{t('pages.alerts.show_all')}</Button>{/snippet}
    </EmptyState>
  {:else}
    <EmptyState icon="alerts" title={t('pages.alerts.all_empty')} body={t('pages.alerts.open_empty_body')} />
  {/if}
</Card>

<style>
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    margin: calc(-1 * var(--sp-2)) 0 var(--sp-2);
    min-height: 32px;
  }
  .count {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .list li {
    display: grid;
    grid-template-columns: 116px minmax(0, 1fr) auto;
    gap: var(--sp-2) var(--sp-4);
    align-items: start;
    padding: var(--sp-4) 0;
  }
  .list li + li {
    border-top: 1px solid var(--line);
  }
  .sev {
    padding-top: 1px;
  }
  .title {
    font-weight: var(--fw-semibold);
  }
  .read .title {
    font-weight: var(--fw-medium);
    color: var(--ink-2);
  }
  .msg {
    margin-top: 2px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 72ch;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1) var(--sp-3);
    margin-top: var(--sp-1);
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .act {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--sp-2);
  }
  @media (max-width: 720px) {
    .list li {
      grid-template-columns: minmax(0, 1fr);
    }
    .act {
      justify-content: flex-start;
    }
  }
</style>
