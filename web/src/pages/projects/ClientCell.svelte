<script lang="ts">
  import type { Project } from '../../lib/api/contract';
  import { t } from '../../lib/i18n/index.svelte';
  import Icon from '../../lib/ui/Icon.svelte';

  interface Props {
    project: Project;
    listId: string;
    onsave: (client: string) => Promise<boolean>;
  }
  let { project, listId, onsave }: Props = $props();
  let editing = $state(false);
  let value = $state('');
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();

  function start(e: Event) {
    e.stopPropagation();
    value = project.client ?? '';
    editing = true;
    queueMicrotask(() => input?.select());
  }
  async function commit() {
    if (!editing || busy) return;
    const next = value.trim();
    if (next === (project.client ?? '')) {
      editing = false;
      return;
    }
    busy = true;
    const ok = await onsave(next);
    busy = false;
    if (ok) editing = false;
  }
</script>

{#if editing}
  <form
    class="edit"
    onsubmit={(e) => {
      e.preventDefault();
      void commit();
    }}
  >
    <label class="sr-only" for="client-{project.id}">{t('pages.projects.edit_client', { name: project.name })}</label>
    <input
      bind:this={input}
      id="client-{project.id}"
      list={listId}
      bind:value
      placeholder={t('pages.projects.client_placeholder')}
      disabled={busy}
      onblur={() => void commit()}
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          editing = false;
        }
      }}
    />
  </form>
{:else}
  <button type="button" class="cell" class:empty={!project.client} onclick={start} aria-label={t('pages.projects.edit_client', { name: project.name })}>
    <span>{project.client ?? t('pages.projects.assign')}</span>
    <Icon name={project.client ? 'pencil' : 'plus'} size={14} />
  </button>
{/if}

<style>
  .cell {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    max-width: 100%;
    min-height: 30px;
    padding: 0 var(--sp-2);
    margin-left: calc(-1 * var(--sp-2));
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .cell span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cell :global(svg) {
    flex: none;
    color: var(--ink-3);
    opacity: 0;
  }
  .cell:hover,
  .cell:focus-visible {
    border-color: var(--line-strong);
    background: var(--surface);
  }
  .cell:hover :global(svg),
  .cell:focus-visible :global(svg),
  .empty :global(svg) {
    opacity: 1;
  }
  .empty {
    color: var(--accent);
    font-weight: var(--fw-medium);
  }
  .edit input {
    width: min(220px, 100%);
    min-height: 32px;
    padding: 0 var(--sp-2);
    border: 1px solid var(--accent);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--ink);
    font: inherit;
  }
</style>
