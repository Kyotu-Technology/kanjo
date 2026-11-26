<script lang="ts">
  import type { AuditEntry } from '../../lib/api/contract';
  import { queries } from '../../lib/api/queries';
  import { useQuery } from '../../lib/api/query.svelte';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import { describeError } from '../../lib/stores/app.svelte';
  import { exportView } from '../../lib/shell/actions';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';

  const PAGE = 12;
  let offset = $state(0);
  const page = useQuery(() => queries.audit({ limit: PAGE, offset }));
  const d = $derived(page.data);

  function label(map: string, key: string): string {
    const k = `${map}.${key.replace(/[^a-z0-9]+/gi, '_')}`;
    const v = tk(k);
    return v === k ? key : v;
  }

  function details(e: AuditEntry): string {
    const v = e.detail;
    if (!v || typeof v !== 'object' || Array.isArray(v)) return v === null || v === undefined ? '' : String(v);
    return Object.entries(v)
      .map(([k, x]) => `${k}: ${Array.isArray(x) ? x.join(', ') : typeof x === 'object' && x !== null ? JSON.stringify(x) : String(x)}`)
      .join('; ');
  }

  const columns = $derived<Column<AuditEntry>[]>([
    { key: 'when', label: t('system.audit.when'), width: '150px' },
    { key: 'action', label: t('system.audit.action') },
    { key: 'details', label: t('system.audit.details'), optional: true },
    { key: 'who', label: t('system.audit.who'), optional: true },
  ]);
</script>

<Card id="audit" title={t('system.audit.title')} subtitle={t('system.audit.subtitle')} refreshing={page.refreshing} flush>
  {#if d}
    <p class="chain" class:bad={!d.chain_ok} role={d.chain_ok ? undefined : 'alert'}>
      <Icon name={d.chain_ok ? 'lock' : 'warning'} size={18} />
      <span>{d.chain_ok ? t('system.audit.chain_ok') : t('system.audit.chain_bad')}</span>
    </p>
    {#if d.items.length}
      <DataTable caption={t('system.audit.title')} rows={d.items} {columns} rowKey={(e) => String(e.id)} dense>
        {#snippet cell(e, col)}
          {#if col.key === 'when'}<span class="num">{fmt.dateTime(e.ts)}</span>
          {:else if col.key === 'action'}
            <span class="act">{label('system.audit.actions', e.action)}</span>
            <code class="target">{e.target}</code>
          {:else if col.key === 'details'}<span class="det">{details(e)}</span>
          {:else}{label('system.audit.actor', e.actor)}{/if}
        {/snippet}
      </DataTable>
      <nav class="pager" aria-label={t('system.audit.title')}>
        <Button size="sm" variant="ghost" icon="download" onclick={() => exportView('audit')}>{t('system.audit.export')}</Button>
        <span class="range">{t('system.audit.page', { from: fmt.number(offset + 1), to: fmt.number(Math.min(offset + PAGE, d.total)), total: fmt.number(d.total) })}</span>
        <Button size="sm" icon="chevron_left" disabled={offset === 0} onclick={() => (offset = Math.max(0, offset - PAGE))}>{t('system.audit.prev')}</Button>
        <Button size="sm" disabled={offset + PAGE >= d.total} onclick={() => (offset += PAGE)}>{t('system.audit.next')}<Icon name="chevron_right" size={16} /></Button>
      </nav>
    {:else}
      <div class="pad"><EmptyState compact icon="info" title={t('system.audit.empty')} /></div>
    {/if}
  {:else if page.error}
    <p class="pad err">{describeError(page.error)}</p>
  {:else}
    <div class="pad"><Skeleton lines={6} height="1.3em" /></div>
  {/if}
</Card>

<style>
  .chain {
    display: flex;
    gap: var(--sp-3);
    margin: 0 var(--sp-5) var(--sp-4);
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    background: var(--good-soft);
    color: var(--good-ink);
    font-size: var(--fs-sm);
  }
  .chain.bad {
    background: var(--critical-soft);
    color: var(--critical-ink);
    font-weight: var(--fw-medium);
  }
  .chain :global(svg) {
    flex: none;
  }
  .act {
    display: block;
    font-weight: var(--fw-medium);
  }
  .target {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .det {
    color: var(--ink-2);
    font-size: var(--fs-xs);
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
  }
  .pager {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--sp-2);
    padding: var(--sp-3) var(--sp-5);
    border-top: 1px solid var(--line);
  }
  .pager {
    flex-wrap: wrap;
  }
  .range {
    margin-right: auto;
    margin-left: var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .pad {
    padding: 0 var(--sp-5) var(--sp-5);
  }
  .err {
    color: var(--critical-ink);
  }
</style>
