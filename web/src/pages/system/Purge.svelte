<script lang="ts">
  import type { PurgeReport, PurgeRequest } from '../../lib/api/contract';
  import { queries } from '../../lib/api/queries';
  import { useQuery } from '../../lib/api/query.svelte';
  import { fmt, t, tn } from '../../lib/i18n/index.svelte';
  import { api, cache, describeError } from '../../lib/stores/app.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import Select from '../../lib/ui/Select.svelte';
  import { around } from '../settings/saver.svelte';

  type Mode = 'project' | 'session' | 'before' | 'everything';
  const MODES: Mode[] = ['project', 'session', 'before', 'everything'];
  const uid = $props.id();
  const projects = useQuery(() => queries.projects());
  const sessions = useQuery(() => queries.sessions({ limit: 200, sort: 'recent' }));
  let mode = $state<Mode>('project');
  let projectId = $state<number>(0);
  let sessionId = $state('');
  let date = $state('');
  let open = $state(false);
  let typed = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let report = $state<PurgeReport | null>(null);

  const plist = $derived(projects.data ?? []);
  const slist = $derived(sessions.data?.items ?? []);
  $effect(() => {
    if (!plist.some((p) => p.id === projectId) && plist[0]) projectId = plist[0].id;
  });
  $effect(() => {
    if (!slist.some((s) => s.id === sessionId) && slist[0]) sessionId = slist[0].id;
  });
  const project = $derived(plist.find((p) => p.id === projectId));
  const session = $derived(slist.find((s) => s.id === sessionId));
  const beforeTs = $derived(date ? new Date(`${date}T00:00:00`).getTime() : NaN);
  const today = new Date().toISOString().slice(0, 10);
  const word = $derived(t('system.purge.word'));
  const ready = $derived(
    mode === 'project' ? !!project : mode === 'session' ? !!session : mode === 'before' ? Number.isFinite(beforeTs) : plist.length > 0 || slist.length > 0,
  );
  const allRequests = $derived(plist.reduce((n, p) => n + p.usage_total.requests, 0));
  const allSessions = $derived(plist.reduce((n, p) => n + p.sessions, 0));

  function request(): PurgeRequest {
    return {
      project: mode === 'project' ? projectId : null,
      session: mode === 'session' ? sessionId : null,
      before: mode === 'before' ? beforeTs : null,
      everything: mode === 'everything',
    };
  }

  function review() {
    typed = '';
    error = null;
    report = null;
    open = true;
  }

  async function purge() {
    busy = true;
    error = null;
    try {
      report = await api().purge(request());
      cache.invalidate(['usage', 'sessions', 'projects', 'limits', 'alerts', 'system', 'budgets', 'ingest']);
    } catch (e) {
      error = t('system.purge.failed', { message: describeError(e) });
    } finally {
      busy = false;
    }
  }

  const REPORT_KEYS = ['requests', 'sessions', 'aggregates', 'limit_samples', 'alerts', 'quarantine'] as const;
</script>

<Card id="purge" title={t('system.purge.title')} subtitle={t('system.purge.subtitle')}>
  <fieldset>
    <legend class="label-text">{t('system.purge.choose')}</legend>
    <div class="modes">
      {#each MODES as m (m)}
        <label class="mode" class:selected={mode === m} class:danger={m === 'everything'}>
          <input type="radio" name="purge-{uid}" value={m} checked={mode === m} onchange={() => (mode = m)} />
          <span class="name">{t(`system.purge.${m}`)}</span>
          <span class="help">{t(`system.purge.${m}_help`)}</span>
        </label>
      {/each}
    </div>
  </fieldset>

  <div class="pick">
    {#if mode === 'project'}
      {#if plist.length}
        <Select label={t('system.purge.pick_project')} bind:value={projectId} options={plist.map((p) => ({ value: p.id, label: p.name }))} />
      {:else}<p class="hint">{t('system.purge.no_projects')}</p>{/if}
    {:else if mode === 'session'}
      {#if slist.length}
        <Select
          label={t('system.purge.pick_session')}
          bind:value={sessionId}
          options={slist.map((s) => ({ value: s.id, label: `${s.title ?? s.id.slice(0, 8)}, ${s.project}, ${fmt.date(s.last_at, 'short')}` }))}
        />
      {:else}<p class="hint">{t('system.purge.no_sessions')}</p>{/if}
    {:else if mode === 'before'}
      <div class="field">
        <label class="label" for="date-{uid}">{t('system.purge.pick_date')}</label>
        <input id="date-{uid}" class="input" type="date" max={today} bind:value={date} />
      </div>
    {/if}
  </div>
  <div class="form-foot">
    <Button variant="danger" icon="warning" disabled={!ready} onclick={review}>{t('system.purge.review')}</Button>
  </div>
</Card>

<Dialog bind:open title={report ? t('system.purge.done_title') : t('system.purge.dialog_title')} size="md">
  {#if report}
    <p>{t('system.purge.done_body')}</p>
    <dl class="report">
      {#each REPORT_KEYS as k (k)}
        <div><dt>{t(`system.purge.report.${k}`)}</dt><dd class="num">{fmt.number(report[k])}</dd></div>
      {/each}
    </dl>
  {:else}
    <h3 class="h">{t('system.purge.removes')}</h3>
    <p class="what">
      {#if mode === 'project' && project}
        {@const [a, b] = around((m) =>
          t('system.purge.removes_project', {
            name: project.name,
            sessions: tn('units.sessions', project.sessions),
            requests: tn('units.requests', project.usage_total.requests),
            cost: m,
          }),
        )}
        {a}<Money usd={project.usage_total.cost_usd} confidence="lower_bound" size="sm" badge="seal" />{b}
      {:else if mode === 'session' && session}
        {@const [a, b] = around((m) =>
          t('system.purge.removes_session', {
            name: session.title ?? session.id,
            date: fmt.date(session.started_at),
            requests: tn('units.requests', session.usage.requests),
            cost: m,
          }),
        )}
        {a}<Money usd={session.usage.cost_usd} confidence="lower_bound" size="sm" badge="seal" />{b}
      {:else if mode === 'before'}
        {t('system.purge.removes_before', { date: fmt.date(beforeTs, 'long') })}
      {:else}
        {t('system.purge.removes_everything', { requests: tn('units.requests', allRequests), sessions: tn('units.sessions', allSessions) })}
      {/if}
    </p>
    <p class="parts">{t('system.purge.removes_parts')}</p>
    <h3 class="h">{t('system.purge.keeps')}</h3>
    <p class="parts">{t('system.purge.keeps_list')}</p>
    <p class="parts">{t('system.purge.no_reimport')}</p>
    <p class="note warn"><Icon name="warning" size={16} /><span>{t('system.purge.backup_hint')}</span></p>
    <label class="confirm">
      <span>{t('system.purge.type_word', { word })}</span>
      <input class="input" autocomplete="off" spellcheck="false" bind:value={typed} aria-describedby="word-{uid}" />
      <code id="word-{uid}" class="sr-only">{word}</code>
    </label>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  {/if}
  {#snippet actions()}
    {#if report}
      <Button variant="primary" onclick={() => (open = false)}>{t('common.done')}</Button>
    {:else}
      <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
      <Button variant="danger" busy={busy} disabled={typed.trim().toLocaleUpperCase() !== word || busy} onclick={purge}>{busy ? t('system.purge.working') : t('system.purge.confirm')}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .modes {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--sp-2);
    margin-top: var(--sp-2);
  }
  .mode {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--sp-3) var(--sp-4);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    cursor: pointer;
  }
  .mode input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .mode:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .mode.selected {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
    background: var(--accent-soft);
  }
  .mode.danger.selected {
    border-color: var(--critical);
    box-shadow: inset 0 0 0 1px var(--critical);
    background: var(--critical-soft);
  }
  .name {
    font-weight: var(--fw-semibold);
  }
  .help {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    line-height: var(--lh-snug);
  }
  .pick {
    margin-top: var(--sp-4);
    max-width: 420px;
  }
  .pick :global(select) {
    width: 100%;
  }
  .h {
    margin-top: var(--sp-4);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
    color: var(--ink-2);
  }
  .h:first-child {
    margin-top: 0;
  }
  .what {
    margin-top: var(--sp-1);
    font-weight: var(--fw-semibold);
  }
  .parts {
    margin-top: var(--sp-1);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .note {
    margin-top: var(--sp-4);
  }
  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    margin-top: var(--sp-4);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .err {
    margin-top: var(--sp-3);
  }
  .report {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--sp-3);
    margin-top: var(--sp-4);
  }
  .report dt {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .report dd {
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
  }
  @media (max-width: 980px) {
    .modes {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 480px) {
    .modes,
    .report {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
