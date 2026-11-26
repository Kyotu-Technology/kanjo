<script lang="ts">
  import type { QuarantineRow } from '../../lib/api/contract';
  import { fmt, t, tk, tn } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';

  let { rows, refreshing = false }: { rows: QuarantineRow[]; refreshing?: boolean } = $props();
  const total = $derived(rows.reduce((n, r) => n + r.count, 0));
  let sortKey = $state('last');
  let sortDir = $state<'asc' | 'desc'>('desc');

  function reason(r: string): string {
    const key = `system.quarantine.reasons.${r.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_|_$/g, '')}`;
    const v = tk(key);
    return v === key ? r : v;
  }

  const columns = $derived<Column<QuarantineRow>[]>([
    { key: 'type', label: t('system.quarantine.type'), value: (r) => r.record_type },
    { key: 'reason', label: t('system.quarantine.reason'), value: (r) => reason(r.reason) },
    { key: 'version', label: t('system.quarantine.version'), optional: true, value: (r) => r.cc_version },
    { key: 'count', label: t('system.quarantine.count'), align: 'end', value: (r) => r.count },
    { key: 'last', label: t('system.quarantine.last'), optional: true, value: (r) => r.last_seen },
  ]);
</script>

<Card id="quarantine" title={t('system.quarantine.title')} subtitle={total ? tn('system.quarantine.total', total) : undefined} {refreshing} flush>
  <p class="explain">{t('system.quarantine.explain')}</p>
  {#if rows.length}
    <DataTable caption={t('system.quarantine.title')} {rows} {columns} rowKey={(r) => r.record_type + r.signature + r.reason} bind:sortKey bind:sortDir dense>
      {#snippet cell(r, col)}
        {#if col.key === 'type'}
          <code class="type">{r.record_type}</code>
          <span class="sig" title={r.signature}>{t('system.quarantine.fields', { fields: r.signature.split(',').join(', ') })}</span>
        {:else if col.key === 'reason'}{reason(r.reason)}
        {:else if col.key === 'version'}{r.cc_version}
        {:else if col.key === 'count'}<span class="num">{fmt.number(r.count)}</span>
        {:else}{fmt.dateTime(r.last_seen)}{/if}
      {/snippet}
    </DataTable>
  {:else}
    <div class="empty"><EmptyState compact icon="check" title={t('system.quarantine.empty')} /></div>
  {/if}
</Card>

<style>
  .explain {
    max-width: 78ch;
    padding: 0 var(--sp-5) var(--sp-4);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .type {
    display: block;
    font-weight: var(--fw-semibold);
  }
  .sig {
    display: block;
    max-width: 46ch;
    color: var(--ink-3);
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
  }
  .empty {
    padding: 0 var(--sp-5) var(--sp-5);
  }
</style>
