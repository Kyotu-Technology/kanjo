<script lang="ts">
  import type { Alert } from '../../lib/api/contract';
  import { fmt, t, tn } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import { api, app, cache, describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Severity from '../../lib/ui/Severity.svelte';

  interface Props {
    alerts: Alert[];
    refreshing?: boolean;
  }
  let { alerts, refreshing = false }: Props = $props();
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

<Card title={t('dashboard.alerts.title')} {refreshing}>
  {#snippet actions()}
    {#if alerts.length > 1}<Button size="sm" variant="ghost" busy={busy === 'all'} onclick={() => ack(null)}>{t('dashboard.alerts.ack_all')}</Button>{/if}
  {/snippet}
  {#if alerts.length}
    <ul class="list">
      {#each alerts as a (a.id)}
        <li>
          <Severity severity={a.severity} />
          <div class="text">
            <p class="title">{a.title}</p>
            <p class="msg">{a.message}</p>
            <p class="meta">
              <span>{fmt.ago(a.ts)}</span>
              <button type="button" class="ack" disabled={busy !== null} onclick={() => ack(a.id)}>{t('dashboard.alerts.ack')}</button>
            </p>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState compact icon="check" title={t('dashboard.alerts.empty')} />
  {/if}
  {#snippet footer()}
    <a href={href('/alerts')}>{t('dashboard.alerts.all')}</a>
  {/snippet}
</Card>

<style>
  .list {
    display: flex;
    flex-direction: column;
  }
  li {
    display: flex;
    gap: var(--sp-3);
    padding: var(--sp-3) 0;
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  li:first-child {
    padding-top: 0;
  }
  li :global(.sev) {
    margin-top: 1px;
    align-self: flex-start;
  }
  .text {
    min-width: 0;
  }
  .title {
    font-weight: var(--fw-semibold);
    font-size: var(--fs-sm);
  }
  .msg {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    margin-top: 2px;
  }
  .meta {
    display: flex;
    gap: var(--sp-3);
    margin-top: var(--sp-1);
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .ack {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: inherit;
    font-weight: var(--fw-medium);
  }
  .ack:hover {
    text-decoration: underline;
  }
</style>
