<script lang="ts">
  import type { CacheInsights } from '../../lib/api/contract';
  import ChartCard from '../../lib/charts/ChartCard.svelte';
  import ShareBars from '../../lib/charts/ShareBars.svelte';
  import { fmt, t, tk, tn } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import Card from '../../lib/ui/Card.svelte';
  import DataTable, { type Column } from '../../lib/ui/DataTable.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';
  import { causeKey, causeTip } from './causes';
  import NextStep from './NextStep.svelte';

  interface Props {
    cache: CacheInsights;
    refreshing?: boolean;
  }
  let { cache: c, refreshing = false }: Props = $props();
  const causeName = (cause: string) => {
    const k = causeKey(cause);
    return k ? tk(`pages.insights.cache_box.cause.${k}`) : t('pages.insights.cache_box.cause.other', { cause: cause.replace(/_/g, ' ') });
  };
  const quality = $derived(c.hit_ratio === null ? 'none' : c.hit_ratio >= 0.8 ? 'good' : c.hit_ratio >= 0.6 ? 'fair' : 'poor');
  const totalMisses = $derived(c.miss_causes.reduce((a, x) => a + x.count, 0));
  const causes = $derived([...c.miss_causes].sort((a, b) => b.count - a.count));
  const topTip = $derived(causes.length ? causeTip(causes[0].cause) : null);
  const byProject = $derived([...c.by_project].sort((a, b) => (b.hit_ratio ?? -1) - (a.hit_ratio ?? -1)));
  let projSort = $state('ratio');
  const projColumns = $derived<Column<(typeof c.by_project)[number]>[]>([
    { key: 'project', label: t('pages.insights.cache_box.col_project'), value: (r) => r.label },
    { key: 'ratio', label: t('pages.insights.cache_box.col_ratio'), align: 'end', value: (r) => r.hit_ratio ?? -1 },
    { key: 'read', label: t('pages.insights.cache_box.col_read'), align: 'end', value: (r) => r.cache_read, optional: true },
    { key: 'write', label: t('pages.insights.cache_box.col_write'), align: 'end', value: (r) => r.cache_write, optional: true },
    { key: 'input', label: t('pages.insights.cache_box.col_input'), align: 'end', value: (r) => r.input, optional: true },
  ]);
  const causeColumns = $derived([
    { key: 'cause', label: t('pages.insights.cache_box.col_cause') },
    { key: 'count', label: t('pages.insights.cache_box.causes_label'), align: 'end' as const },
  ]);
  let missSort = $state('when');
  let missDir = $state<'asc' | 'desc'>('desc');
  const missColumns = $derived<Column<(typeof c.recent_misses)[number]>[]>([
    { key: 'when', label: t('pages.insights.cache_box.col_when'), value: (m) => m.ts },
    { key: 'session', label: t('pages.insights.cache_box.col_session'), value: (m) => m.project },
    { key: 'cause', label: t('pages.insights.cache_box.col_cause') },
    { key: 'detail', label: t('pages.insights.cache_box.col_detail'), optional: true },
  ]);
  const details = (m: (typeof c.recent_misses)[number]) => {
    const out: string[] = [];
    if (m.tools_added !== null || m.tools_removed !== null)
      out.push(t('pages.insights.cache_box.tools_delta', { added: fmt.number(m.tools_added ?? 0), removed: fmt.number(m.tools_removed ?? 0) }));
    if (m.system_char_delta !== null) out.push(t('pages.insights.cache_box.system_delta', { chars: fmt.number(Math.abs(m.system_char_delta)) }));
    for (const ch of m.correlated_changes) out.push(t('pages.insights.cache_box.after_change', { target: ch.target }));
    return out;
  };
  const kindName = (k: string) => (['claude_md', 'settings', 'plugins', 'tools'].includes(k) ? tk(`pages.insights.cache_box.kind.${k}`) : k);
</script>

<div class="cache">
  <div class="span-5">
    <Card title={t('pages.insights.cache_box.title')} subtitle={t('pages.insights.cache_box.causes_sub')} {refreshing}>
      {#snippet actions()}<InlineHelp text={t('pages.insights.cache_help')} />{/snippet}
      {#if c.hit_ratio !== null}
        <p class="big">{fmt.ratio(c.hit_ratio)}</p>
        <p class="sentence">{t('pages.insights.cache_box.sentence', { pct: fmt.ratio(c.hit_ratio) })}</p>
        <p class="quality">{tk(`pages.insights.cache_box.${quality}`)}</p>
        <NextStep text={t('pages.insights.cache_box.do')} />
      {:else}
        <EmptyState compact icon="info" title={t('pages.insights.cache_box.none')} />
      {/if}
    </Card>
  </div>
  <div class="span-7">
    <ChartCard title={t('pages.insights.cache_box.by_project')} subtitle={t('pages.insights.cache_box.causes_sub')} {refreshing}>
      {#snippet chart()}
        {#if byProject.length}
          <ShareBars
            label={t('pages.insights.cache_box.by_project')}
            shareFormat={() => ''}
            rows={byProject.map((r) => ({ key: r.key, label: r.label, value: r.hit_ratio === null ? t('common.unknown') : fmt.ratio(r.hit_ratio), share: r.hit_ratio ?? 0 }))}
          />
        {:else}
          <EmptyState compact icon="info" title={t('pages.insights.cache_box.none')} />
        {/if}
      {/snippet}
      {#snippet table()}
        <DataTable caption={t('pages.insights.cache_box.by_project')} rows={c.by_project} columns={projColumns} rowKey={(r) => r.key} bind:sortKey={projSort} dense>
          {#snippet cell(r, col)}
            {#if col.key === 'project'}{r.label}
            {:else if col.key === 'ratio'}{r.hit_ratio === null ? t('common.unknown') : fmt.ratio(r.hit_ratio)}
            {:else if col.key === 'read'}{fmt.compact(r.cache_read)}
            {:else if col.key === 'write'}{fmt.compact(r.cache_write)}
            {:else}{fmt.compact(r.input)}{/if}
          {/snippet}
        </DataTable>
      {/snippet}
    </ChartCard>
  </div>
  <div class="span-6">
    <ChartCard title={t('pages.insights.cache_box.causes_title')} subtitle={t('pages.insights.cache_box.causes_sub')} {refreshing}>
      {#snippet chart()}
        {#if causes.length}
          <ShareBars
            label={t('pages.insights.cache_box.causes_label')}
            shareFormat={(s) => fmt.ratio(s)}
            rows={causes.map((x) => ({ key: x.cause, label: causeName(x.cause), value: tn('pages.insights.cache_box.misses', x.count), share: totalMisses ? x.count / totalMisses : 0 }))}
          />
          {#if topTip}<NextStep text={tk(`pages.insights.cache_box.tip.${topTip}`)} />{/if}
        {:else}
          <EmptyState compact icon="check" title={t('pages.insights.cache_box.causes_none')} />
        {/if}
      {/snippet}
      {#snippet table()}
        <DataTable caption={t('pages.insights.cache_box.causes_title')} rows={causes} columns={causeColumns} rowKey={(x) => x.cause} dense>
          {#snippet cell(x, col)}{col.key === 'cause' ? causeName(x.cause) : fmt.number(x.count)}{/snippet}
        </DataTable>
      {/snippet}
    </ChartCard>
  </div>
  <div class="span-6">
    <Card title={t('pages.insights.cache_box.breakers_title')} {refreshing}>
      {#if c.breakers.length}
        <ul class="breakers">
          {#each c.breakers as b (b.change.ts + b.change.target)}
            <li>
              <span class="kind">{kindName(b.change.kind)}</span>
              <div>
                <p class="what">{t('pages.insights.cache_box.breaker', { target: b.change.target, when: fmt.ago(b.change.ts) })}</p>
                <p class="after">
                  {t('pages.insights.cache_box.breaker_after', {
                    misses: tn('pages.insights.cache_box.breaker_misses', b.misses_after),
                    sessions: tn('pages.insights.cache_box.breaker_sessions', b.sessions_affected),
                  })}
                </p>
              </div>
            </li>
          {/each}
        </ul>
        <NextStep text={t('pages.insights.cache_box.breakers_do')} />
      {:else}
        <EmptyState compact icon="check" title={t('pages.insights.cache_box.breakers_none')} />
      {/if}
    </Card>
  </div>
  <div class="span-12">
    <Card title={t('pages.insights.cache_box.misses_title')} flush {refreshing}>
      <DataTable
        caption={t('pages.insights.cache_box.misses_title')}
        rows={c.recent_misses}
        columns={missColumns}
        rowKey={(m) => m.session_id + m.ts}
        bind:sortKey={missSort}
        bind:sortDir={missDir}
        dense
        empty={t('pages.insights.cache_box.misses_none')}
      >
        {#snippet cell(m, col)}
          {#if col.key === 'when'}<span class="nowrap">{fmt.dateTime(m.ts)}</span>
          {:else if col.key === 'session'}<a class="nowrap" href={href(`/sessions/${encodeURIComponent(m.session_id)}`)}>{m.project}</a>
          {:else if col.key === 'cause'}{m.causes.map(causeName).join('; ')}
          {:else}<span class="detail">{details(m).join('. ')}</span>{/if}
        {/snippet}
      </DataTable>
    </Card>
  </div>
</div>

<style>
  .cache {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: var(--sp-5);
  }
  .cache > div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .cache > div > :global(.card) {
    flex: 1;
  }
  .span-12 {
    grid-column: span 12;
  }
  .span-7 {
    grid-column: span 7;
  }
  .span-6 {
    grid-column: span 6;
  }
  .span-5 {
    grid-column: span 5;
  }
  .big {
    font-size: var(--fs-hero);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.03em;
    line-height: 1;
  }
  .sentence {
    margin-top: var(--sp-3);
    font-size: var(--fs-md);
    max-width: 46ch;
  }
  .quality {
    margin-top: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 52ch;
  }
  .breakers li {
    display: grid;
    grid-template-columns: 112px minmax(0, 1fr);
    gap: var(--sp-3);
    padding: var(--sp-3) 0;
  }
  .breakers li + li {
    border-top: 1px solid var(--line);
  }
  .kind {
    align-self: start;
    justify-self: start;
    padding: 1px 8px;
    border-radius: var(--r-pill);
    background: var(--surface-3);
    color: var(--ink-2);
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
  }
  .what {
    font-weight: var(--fw-medium);
    overflow-wrap: anywhere;
  }
  .after {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .nowrap {
    white-space: nowrap;
  }
  .detail {
    color: var(--ink-2);
  }
  @media (max-width: 640px) {
    .breakers li {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-1);
    }
  }
  @media (max-width: 1180px) {
    .span-7,
    .span-6,
    .span-5 {
      grid-column: span 12;
    }
  }
  @media (max-width: 860px) {
    .cache {
      gap: var(--sp-4);
    }
  }
</style>
