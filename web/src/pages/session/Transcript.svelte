<script lang="ts">
  import type { Transcript } from '../../lib/api/contract';
  import { asApiError } from '../../lib/api/errors';
  import { modelLabel, modelTitle } from '../../lib/charts/series';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import { api, app, describeError } from '../../lib/stores/app.svelte';
  import Banner from '../../lib/ui/Banner.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';

  interface Props {
    id: string;
  }
  let { id }: Props = $props();
  const enabled = $derived(!!app.settings?.privacy.conversation_viewer);
  let data = $state.raw<Transcript | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      data = await api().transcript(id);
    } catch (e) {
      error = describeError(asApiError(e));
    } finally {
      loading = false;
    }
  }
</script>

<Card title={t('pages.session.transcript.title')}>
  {#snippet actions()}
    {#if enabled}<span class="ro"><Icon name="lock" size={14} />{t('pages.session.transcript.read_only')}</span>{/if}
  {/snippet}
  {#if !enabled}
    <div class="off">
      <span class="ico"><Icon name="lock" size={20} /></span>
      <div>
        <p class="h">{t('pages.session.transcript.off_title')}</p>
        <p class="b">{t('pages.session.transcript.off_body')}</p>
        <Button href={href('/settings', { section: 'privacy' })} icon="settings">{t('pages.session.transcript.off_action')}</Button>
      </div>
    </div>
  {:else if !data}
    <p class="b">{t('pages.session.transcript.on_body')}</p>
    <p class="audit"><Icon name="info" size={16} /><span>{t('pages.session.transcript.audit')}</span></p>
    {#if error}<div class="err"><Banner tone="warning" title={t('common.error_title')} body={t('pages.session.transcript.failed', { message: error })} /></div>{/if}
    {#if loading}
      <p class="loading" aria-live="polite">{t('pages.session.transcript.loading')}</p>
      <Skeleton lines={4} height="1.2em" />
    {:else}
      <Button variant="primary" icon="sessions" onclick={load}>{t('pages.session.transcript.load')}</Button>
    {/if}
  {:else}
    <div class="bar">
      <p class="audit"><Icon name="info" size={16} /><span>{t('pages.session.transcript.audit')}</span></p>
      <Button variant="ghost" size="sm" icon="x" onclick={() => (data = null)}>{t('pages.session.transcript.hide')}</Button>
    </div>
    {#if !data.messages.length}
      <p class="b">{t('pages.session.transcript.empty')}</p>
    {:else}
      <ol class="msgs">
        {#each data.messages as m, i (i)}
          <li class="msg r-{m.role} k-{m.kind}">
            <p class="meta">
              <strong>{tk(`pages.session.transcript.role.${m.role}`)}</strong>
              {#if m.kind !== 'text'}<span class="kind">{tk(`pages.session.transcript.kind.${m.kind}`)}{#if m.tool}: {m.tool}{/if}</span>{/if}
              {#if m.model}<span class="model" title={modelTitle(m.model)}>{modelLabel(m.model)}</span>{/if}
              {#if m.ts !== null}<time>{fmt.time(m.ts)}</time>{/if}
            </p>
            {#if m.kind === 'thinking'}
              <details>
                <summary>{tk('pages.session.transcript.kind.thinking')}</summary>
                <p class="text">{m.text}</p>
              </details>
            {:else}
              <p class="text" class:mono={m.kind === 'tool_use' || m.kind === 'tool_result'}>{m.text}</p>
            {/if}
          </li>
        {/each}
      </ol>
      {#if data.truncated}<p class="b">{t('pages.session.transcript.truncated')}</p>{/if}
    {/if}
  {/if}
</Card>

<style>
  .ro {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .off {
    display: flex;
    gap: var(--sp-4);
    align-items: flex-start;
  }
  .ico {
    display: inline-flex;
    padding: var(--sp-2);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--ink-2);
    flex: none;
  }
  .h {
    font-weight: var(--fw-semibold);
  }
  .b {
    margin: var(--sp-1) 0 var(--sp-4);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 68ch;
  }
  .audit {
    display: flex;
    gap: var(--sp-2);
    align-items: flex-start;
    margin-bottom: var(--sp-4);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .audit :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--accent);
  }
  .bar {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--sp-3);
  }
  .bar .audit {
    margin-bottom: var(--sp-3);
  }
  .err {
    margin-bottom: var(--sp-3);
  }
  .loading {
    margin-bottom: var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .msgs {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    max-height: 560px;
    overflow-y: auto;
    padding-right: var(--sp-1);
  }
  .msg {
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .msg.r-user.k-text {
    background: var(--accent-soft);
    border-color: transparent;
  }
  .msg.k-tool_use,
  .msg.k-tool_result,
  .msg.k-thinking {
    background: var(--surface-2);
    border-color: transparent;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--sp-2);
    margin-bottom: var(--sp-1);
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .meta strong {
    color: var(--ink);
    font-size: var(--fs-sm);
  }
  time {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: var(--fs-sm);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--ink-2);
  }
  summary {
    cursor: pointer;
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  details .text {
    margin-top: var(--sp-2);
    color: var(--ink-2);
  }
</style>
