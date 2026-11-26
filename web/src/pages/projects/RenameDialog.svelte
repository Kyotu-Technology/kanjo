<script lang="ts">
  import type { Project } from '../../lib/api/contract';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/stores/app.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';

  interface Props {
    open: boolean;
    project: Project | null;
    onsave: (name: string) => Promise<boolean>;
  }
  let { open = $bindable(), project, onsave }: Props = $props();
  let name = $state('');
  let busy = $state(false);
  let error = $state('');
  $effect(() => {
    if (open && project) {
      name = project.name;
      error = '';
    }
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const next = name.trim();
    if (!next) {
      error = t('pages.projects.rename_empty');
      return;
    }
    busy = true;
    const ok = await onsave(next);
    busy = false;
    if (ok) open = false;
  }
</script>

<Dialog bind:open title={t('pages.projects.rename_title', { name: project?.name ?? '' })} description={t('pages.projects.rename_body')} size="sm">
  <form id="rename-project" onsubmit={submit} novalidate>
    <label for="rename-input">{t('pages.projects.rename_label')}</label>
    <input id="rename-input" bind:value={name} maxlength="80" autocomplete="off" aria-invalid={!!error} aria-describedby={error ? 'rename-error' : undefined} />
    {#if error}<p id="rename-error" class="error">{error}</p>{/if}
    {#if app.settings?.privacy.redact_paths}<p class="note">{t('pages.projects.rename_redacted')}</p>{/if}
  </form>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" type="submit" form="rename-project" {busy}>{t('common.save')}</Button>
  {/snippet}
</Dialog>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  label {
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    color: var(--ink-2);
  }
  input {
    min-height: 40px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--ink);
    font: inherit;
  }
  input[aria-invalid='true'] {
    border-color: var(--critical);
  }
  .error {
    color: var(--critical-ink);
    font-size: var(--fs-sm);
  }
  .note {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
