<script lang="ts">
  import type { PrivacySettings, Settings } from '../../lib/api/contract';
  import { t } from '../../lib/i18n/index.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Switch from '../../lib/ui/Switch.svelte';
  import SaveStatus from './SaveStatus.svelte';
  import { Saver } from './saver.svelte';

  let { s }: { s: Settings } = $props();
  const saver = new Saver();
  type Choice = 'titles' | 'redact' | 'viewer';
  const FIELD: Record<Choice, keyof PrivacySettings> = { titles: 'session_titles', redact: 'redact_paths', viewer: 'conversation_viewer' };
  const p = $derived(s.privacy);
  let pending = $state<{ choice: Choice; value: boolean } | null>(null);
  let open = $state(false);

  /** Switching titles or the viewer on, or redaction off, shows more; that needs a confirmation. */
  const relaxes = (c: Choice, v: boolean) => (c === 'redact' ? !v : v);

  function request(c: Choice, v: boolean) {
    if (relaxes(c, v)) {
      pending = { choice: c, value: v };
      open = true;
    } else void apply(c, v);
  }

  function apply(c: Choice, v: boolean) {
    return saver.save((x) => ({ ...x, privacy: { ...x.privacy, [FIELD[c]]: v } }), ['sessions', 'projects', 'usage', `privacy`]);
  }

  function confirm() {
    const q = pending;
    open = false;
    pending = null;
    if (q) void apply(q.choice, q.value);
  }
</script>

<Card id="privacy" title={t('settings.privacy.title')} subtitle={t('settings.privacy.subtitle')}>
  {#snippet actions()}<SaveStatus state={saver.state} />{/snippet}
  <p class="note sure"><Icon name="lock" size={16} /><span>{t('settings.privacy.never')}</span></p>
  <div class="rows">
    <Switch bind:checked={() => p.session_titles, (v) => request('titles', v)} label={t('settings.privacy.titles')} description={t('settings.privacy.titles_help')} />
    <Switch bind:checked={() => p.redact_paths, (v) => request('redact', v)} label={t('settings.privacy.redact')} description={t('settings.privacy.redact_help')} />
    <Switch bind:checked={() => p.conversation_viewer, (v) => request('viewer', v)} label={t('settings.privacy.viewer')} description={t('settings.privacy.viewer_help')} />
  </div>
</Card>

<Dialog
  bind:open
  title={pending ? t(`settings.privacy.confirm.${pending.choice}_title`) : ''}
  size="sm"
  onclose={() => (pending = null)}
>
  {#if pending}
    <p class="body">{t(`settings.privacy.confirm.${pending.choice}_body`)}</p>
    <p class="undo">{t('settings.privacy.confirm.undo')}</p>
  {/if}
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" onclick={confirm}>{pending ? t(`settings.privacy.confirm.${pending.choice}_action`) : ''}</Button>
  {/snippet}
</Dialog>

<style>
  .sure {
    margin-bottom: var(--sp-3);
  }
  .rows > :global(*) {
    padding: var(--sp-3) 0;
  }
  .rows > :global(* + *) {
    border-top: 1px solid var(--line);
  }
  .body {
    color: var(--ink);
  }
  .undo {
    margin-top: var(--sp-3);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
