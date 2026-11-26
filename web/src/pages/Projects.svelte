<script lang="ts">
  import type { Project, ProjectPatch } from '../lib/api/contract';
  import { asApiError } from '../lib/api/errors';
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import ChartCard from '../lib/charts/ChartCard.svelte';
  import ShareBars from '../lib/charts/ShareBars.svelte';
  import { fmt, i18n, t, tn } from '../lib/i18n/index.svelte';
  import { toDateInput } from '../lib/range';
  import { href } from '../lib/router';
  import { api, app, cache, describeError } from '../lib/stores/app.svelte';
  import { toasts } from '../lib/stores/toast.svelte';
  import { runExport } from '../lib/shell/actions';
  import { usePageExport, type PageExport } from '../lib/shell/exports.svelte';
  import Answer from '../lib/ui/Answer.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Card from '../lib/ui/Card.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import DataTable, { type Column } from '../lib/ui/DataTable.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import InlineHelp from '../lib/ui/InlineHelp.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Rich, { strong } from '../lib/ui/Rich.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import ClientCell from './projects/ClientCell.svelte';
  import RenameDialog from './projects/RenameDialog.svelte';

  const projects = useQuery(() => queries.projects());
  const list = $derived(projects.data ?? []);
  let sortKey = $state('cost_30d');
  let sortDir = $state<'asc' | 'desc'>('desc');
  let renaming = $state<Project | null>(null);
  let renameOpen = $state(false);
  let clientSort = $state('cost');
  let clientDir = $state<'asc' | 'desc'>('desc');
  const redacted = $derived(!!app.settings?.privacy.redact_paths);
  const total30 = $derived(list.reduce((a, p) => a + p.usage_30d.cost_usd, 0));
  const active30 = $derived(list.filter((p) => p.usage_30d.requests > 0));
  const top = $derived([...list].sort((a, b) => b.usage_30d.cost_usd - a.usage_30d.cost_usd)[0]);
  const unassigned = $derived(list.filter((p) => !p.client).length);
  const clients = $derived([...new Set(list.map((p) => p.client).filter((c): c is string => !!c))].sort());
  const byClient = $derived.by(() => {
    const m = new Map<string, { key: string; label: string; cost: number; projects: number; unpriced: number }>();
    for (const p of list) {
      const key = p.client ?? '';
      const row = m.get(key) ?? { key, label: p.client ?? t('pages.projects.no_client'), cost: 0, projects: 0, unpriced: 0 };
      row.cost += p.usage_30d.cost_usd;
      row.unpriced += p.usage_30d.unpriced_requests;
      row.projects += 1;
      m.set(key, row);
    }
    return [...m.values()].sort((a, b) => b.cost - a.cost);
  });
  type ClientRow = (typeof byClient)[number];
  const columns = $derived<Column<Project>[]>([
    { key: 'name', label: t('pages.projects.name'), value: (p) => p.name },
    { key: 'client', label: t('pages.projects.client'), value: (p) => p.client ?? '', header: clientHelp, optional: true },
    { key: 'sessions', label: t('pages.projects.sessions'), align: 'end', optional: true, value: (p) => p.sessions },
    { key: 'last', label: t('pages.projects.last_seen'), optional: true, value: (p) => p.last_seen },
    { key: 'cost_30d', label: t('pages.projects.cost_30d'), align: 'end', value: (p) => p.usage_30d.cost_usd, header: badge },
    { key: 'cost_total', label: t('pages.projects.cost_total'), align: 'end', optional: true, value: (p) => p.usage_total.cost_usd, header: badge },
    { key: 'actions', label: t('pages.projects.actions'), align: 'end', optional: true },
  ]);
  const clientColumns = $derived<Column<ClientRow>[]>([
    { key: 'label', label: t('pages.projects.client'), value: (r) => r.label },
    { key: 'projects', label: t('pages.projects.title'), align: 'end', value: (r) => r.projects },
    { key: 'cost', label: t('pages.projects.cost_30d'), align: 'end', value: (r) => r.cost, header: badge },
  ]);

  async function patch(p: Project, body: ProjectPatch, done: string): Promise<boolean> {
    try {
      await api().patchProject(p.id, body);
      cache.invalidate(['projects', 'usage']);
      toasts.push('success', done);
      return true;
    } catch (e) {
      toasts.push('error', t('pages.projects.save_failed', { message: describeError(asApiError(e)) }));
      return false;
    }
  }
  const saveClient = (p: Project) => (client: string) =>
    patch(p, { name: null, client }, client ? t('pages.projects.client_saved', { name: p.name, client }) : t('pages.projects.client_cleared', { name: p.name }));
  const saveName = (name: string) => (renaming ? patch(renaming, { name, client: null }, t('pages.projects.renamed', { name })) : Promise.resolve(false));
  const exp: PageExport = { kind: 'json', name: 'projects', data: async () => projects.data ?? api().projects() };
  usePageExport(() => exp);
</script>

{#snippet badge()}<ConfidenceBadge confidence="lower_bound" compact />{/snippet}
{#snippet clientHelp()}<InlineHelp text={t('pages.projects.client_help')} label={t('pages.projects.client')} />{/snippet}

<PageHeader title={t('pages.projects.title')} lede={t('pages.projects.lede')}>
  {#snippet meta()}
    <Button size="sm" icon="download" onclick={() => runExport(exp)}>{t('common.export_json')}</Button>
  {/snippet}
</PageHeader>

{#if redacted}
  <div class="banner">
    <Banner icon="lock" title={t('pages.projects.redacted_title')} body={t('pages.projects.redacted_body')}>
      {#snippet action()}<Button href={href('/settings', { section: 'privacy' })} size="sm">{t('pages.projects.redacted_action')}</Button>{/snippet}
    </Banner>
  </div>
{/if}

{#if projects.error && !projects.data}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(projects.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => projects.refetch()}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else if !projects.data}
  <div aria-busy="true">
    <Skeleton height="1.3rem" width="min(560px, 100%)" />
    <div class="gap"></div>
    <Card><Skeleton lines={6} height="1.4em" /></Card>
  </div>
{:else if !list.length}
  <Card><EmptyState icon="projects" title={t('pages.projects.empty_title')} body={t('pages.projects.empty_body')} /></Card>
{:else}
  <Answer confidence="lower_bound" refreshing={projects.refreshing}>
    <Rich text={tn('pages.projects.answer', active30.length, { money: strong(fmt.money(total30)) })} />
    {#if top && total30 > 0}
      <Rich text={t('pages.projects.answer_top', { name: strong(top.name), money: fmt.money(top.usage_30d.cost_usd), share: fmt.ratio(top.usage_30d.cost_usd / total30) })} />
    {/if}
    {#if unassigned}{tn('pages.projects.answer_unassigned', unassigned)}{:else}{t('pages.projects.answer_all_assigned')}{/if}
  </Answer>

  <datalist id="kanjo-clients">
    {#each clients as c (c)}<option value={c}></option>{/each}
  </datalist>

  <div class="layout">
    <Card flush refreshing={projects.refreshing}>
      <DataTable caption={t('pages.projects.title')} rows={list} {columns} rowKey={(p) => String(p.id)} bind:sortKey bind:sortDir>
        {#snippet cell(p, col)}
          {#if col.key === 'name'}
            <strong class="name">{p.name}</strong>
            {#if p.path}<span class="sub path" title={p.path}>{p.path}</span>{/if}
            <span class="narrow">
              <ClientCell project={p} listId="kanjo-clients" onsave={saveClient(p)} />
              <span class="links">
                <a href={href('/sessions', { project: p.id, from: toDateInput(p.first_seen), to: toDateInput(i18n.now) })}>{tn('units.sessions', p.sessions)}</a>
                <a href={href('/usage', { project: p.id })}>{t('pages.projects.view_usage')}</a>
              </span>
              <Button
                size="sm"
                variant="ghost"
                icon="pencil"
                iconOnly
                title={t('pages.projects.rename')}
                onclick={() => {
                  renaming = p;
                  renameOpen = true;
                }}>{t('pages.projects.rename_title', { name: p.name })}</Button
              >
            </span>
          {:else if col.key === 'client'}
            <ClientCell project={p} listId="kanjo-clients" onsave={saveClient(p)} />
          {:else if col.key === 'sessions'}
            <a href={href('/sessions', { project: p.id, from: toDateInput(p.first_seen), to: toDateInput(i18n.now) })}>{fmt.number(p.sessions)}</a>
          {:else if col.key === 'last'}<span class="nowrap">{fmt.ago(p.last_seen)}</span><span class="sub">{t('pages.projects.since', { date: fmt.date(p.first_seen, 'medium') })}</span>
          {:else if col.key === 'cost_30d'}<Money usd={p.usage_30d.cost_usd} confidence="lower_bound" unpriced={p.usage_30d.unpriced_requests} size="sm" badge="none" />
          {:else if col.key === 'cost_total'}<Money usd={p.usage_total.cost_usd} confidence="lower_bound" unpriced={p.usage_total.unpriced_requests} size="sm" badge="none" />
          {:else}
            <span class="acts">
              <Button size="sm" variant="ghost" href={href('/usage', { project: p.id })}>{t('pages.projects.view_usage')}</Button>
              <Button
                size="sm"
                variant="ghost"
                icon="pencil"
                iconOnly
                title={t('pages.projects.rename')}
                onclick={() => {
                  renaming = p;
                  renameOpen = true;
                }}>{t('pages.projects.rename_title', { name: p.name })}</Button
              >
            </span>
          {/if}
        {/snippet}
      </DataTable>
    </Card>

    <ChartCard title={t('pages.projects.by_client')} subtitle={t('pages.projects.by_client_sub')}>
      {#snippet note()}<ConfidenceBadge confidence="lower_bound" />{/snippet}
      {#snippet chart()}
        <ShareBars
          label={t('pages.projects.by_client')}
          shareFormat={(s) => fmt.ratio(s)}
          rows={byClient.map((r) => ({
            key: r.key || '__none__',
            label: r.label,
            sublabel: tn('pages.projects.count', r.projects),
            value: fmt.money(r.cost),
            share: total30 > 0 ? r.cost / total30 : 0,
            href: r.key ? href('/usage', { client: r.key }) : undefined,
          }))}
        />
      {/snippet}
      {#snippet table()}
        <DataTable caption={t('pages.projects.by_client')} rows={byClient} columns={clientColumns} rowKey={(r) => r.key || '__none__'} bind:sortKey={clientSort} bind:sortDir={clientDir} dense>
          {#snippet cell(r, col)}
            {#if col.key === 'label'}{r.label}{:else if col.key === 'projects'}{fmt.number(r.projects)}{:else}<Money usd={r.cost} confidence="lower_bound" unpriced={r.unpriced} size="sm" badge="none" />{/if}
          {/snippet}
        </DataTable>
      {/snippet}
    </ChartCard>
  </div>
{/if}

<RenameDialog bind:open={renameOpen} project={renaming} onsave={saveName} />

<style>
  .banner {
    margin-bottom: var(--sp-5);
  }
  .gap {
    height: var(--sp-5);
  }
  .layout {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .name {
    font-weight: var(--fw-semibold);
  }
  .sub {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .path {
    max-width: 16ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
  }
  .narrow {
    display: none;
  }
  @media (max-width: 719px) {
    .narrow {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--sp-1) var(--sp-3);
      margin-top: var(--sp-1);
    }
    .links {
      display: inline-flex;
      gap: var(--sp-3);
      font-size: var(--fs-sm);
    }
    .path {
      display: none;
    }
  }
  .nowrap {
    white-space: nowrap;
  }
  .acts {
    display: inline-flex;
    gap: var(--sp-1);
    justify-content: flex-end;
  }
</style>
