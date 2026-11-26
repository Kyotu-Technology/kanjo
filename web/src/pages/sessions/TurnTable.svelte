<script lang="ts">
  import type { Turn } from '../../lib/api/contract';
  import { modelColor, modelLabel } from '../../lib/charts/series';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import { navigate } from '../../lib/stores/router.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import Money from '../../lib/ui/Money.svelte';

  interface Props {
    rows: Turn[];
    caption: string;
    /** False inside a session page, where every row belongs to the same session. */
    linkSessions?: boolean;
  }
  let { rows, caption, linkSessions = true }: Props = $props();
  let sortKey = $state('cost');
  let sortDir = $state<'asc' | 'desc'>('desc');
  const read = (x: Turn) => x.tokens.input + x.tokens.cache_read + x.tokens.cache_write_5m + x.tokens.cache_write_1h;
  const columns = $derived<Column<Turn>[]>([
    { key: 'when', label: t('pages.sessions.turn.when'), value: (x) => x.ts },
    linkSessions
      ? { key: 'session', label: t('pages.sessions.turn.session'), value: (x) => x.project }
      : { key: 'agent', label: t('pages.sessions.turn.agent'), value: (x) => x.agent_type ?? '' },
    { key: 'model', label: t('pages.sessions.turn.model'), optional: true, value: (x) => x.model },
    { key: 'tools', label: t('pages.sessions.turn.tools'), optional: true },
    { key: 'read', label: t('pages.sessions.turn.read'), align: 'end', optional: true, value: read, header: readHelp },
    { key: 'written', label: t('pages.sessions.turn.written'), align: 'end', optional: true, value: (x) => x.tokens.output, header: writtenHelp },
    { key: 'cost', label: t('pages.sessions.turn.cost'), align: 'end', value: (x) => x.cost_usd ?? -1, header: badge },
  ]);
  const link = (x: Turn) => href(`/sessions/${encodeURIComponent(x.session_id)}`);
</script>

{#snippet badge()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}
{#snippet readHelp()}<InlineHelp text={t('pages.sessions.turn.read_help')} label={t('pages.sessions.turn.read')} />{/snippet}
{#snippet writtenHelp()}<InlineHelp text={t('pages.sessions.turn.written_help')} label={t('pages.sessions.turn.written')} />{/snippet}

<DataTable {caption} {rows} {columns} rowKey={(x) => x.message_id} bind:sortKey bind:sortDir onrowclick={linkSessions ? (x) => navigate(link(x)) : undefined}>
  {#snippet cell(x, col)}
    {#if col.key === 'when'}
      {linkSessions ? fmt.dateTime(x.ts) : fmt.time(x.ts)}
      {#if x.stop_reason}<span class="sub">{tk(`pages.sessions.stop.${x.stop_reason}`)}</span>{/if}
    {:else if col.key === 'session'}
      <a class="title" href={link(x)} onclick={(e) => e.stopPropagation()}>{x.project}</a>
      <span class="sub">{x.agent_type ?? t('common.main')}</span>
    {:else if col.key === 'agent'}{x.agent_type ?? t('common.main')}
    {:else if col.key === 'model'}
      <span class="model"><span class="key" style="background:{modelColor(x.model)}"></span>{modelLabel(x.model)}</span>
    {:else if col.key === 'tools'}
      <span class="tools">{#each [...new Set(x.tools)].slice(0, 3) as tool (tool)}<span class="tool">{tool}</span>{/each}</span>
    {:else if col.key === 'read'}{fmt.compact(read(x))}
    {:else if col.key === 'written'}{fmt.compact(x.tokens.output)}
    {:else if x.cost_usd === null}<span class="sub">{t('pages.sessions.turn.no_price')}</span>
    {:else}<Money usd={x.cost_usd} confidence="lower_bound" size="sm" badge="none" />{/if}
  {/snippet}
</DataTable>

<style>
  .title {
    display: block;
    color: var(--ink);
    font-weight: var(--fw-semibold);
    text-decoration: none;
  }
  .title:hover {
    text-decoration: underline;
  }
  .sub {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .model {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  .key {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .tools {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tool {
    padding: 1px 6px;
    border-radius: var(--r-xs);
    background: var(--surface-3);
    color: var(--ink-2);
    font-size: var(--fs-xs);
  }
</style>
