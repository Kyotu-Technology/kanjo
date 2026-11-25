<script lang="ts">
  import type { HookStatus } from '../api/contract';
  import { queries } from '../api/queries';
  import { useQuery } from '../api/query.svelte';
  import { t } from '../i18n/index.svelte';
  import { api, cache, describeError } from '../stores/app.svelte';
  import { toasts } from '../stores/toast.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';

  interface Props {
    onconnected?: (h: HookStatus) => void;
  }
  let { onconnected }: Props = $props();
  const system = useQuery(() => queries.system());
  let busy = $state(false);
  let result = $state<HookStatus | null>(null);
  let error = $state<string | null>(null);

  const hooks = $derived(result ?? system.data?.hooks ?? null);
  const connected = $derived(!!hooks && hooks.statusline_installed && hooks.session_hooks_installed);
  const file = $derived(system.data ? `${system.data.claude_dir.replace(/\/$/, '')}/settings.json` : '~/.claude/settings.json');

  async function connect() {
    busy = true;
    error = null;
    try {
      result = await api().installHooks({ statusline: true, session_hooks: true, service: false });
      cache.invalidate(['system', 'limits', 'usage']);
      toasts.push('success', t('onboarding.connect.success'));
      onconnected?.(result);
    } catch (e) {
      error = t('onboarding.connect.failed', { message: describeError(e) });
    } finally {
      busy = false;
    }
  }
</script>

<div class="connect">
  <ul class="parts">
    <li>
      <span class="glyph"><Icon name="limits" /></span>
      <div>
        <p class="name">{t('onboarding.connect.statusline')}</p>
        <p class="desc">{t('onboarding.connect.statusline_help')}</p>
      </div>
      {#if hooks?.statusline_installed}<span class="ok"><Icon name="check" size={16} strokeWidth={2.2} />{t('pages.system.installed')}</span>{/if}
    </li>
    <li>
      <span class="glyph"><Icon name="sessions" /></span>
      <div>
        <p class="name">{t('onboarding.connect.hooks')}</p>
        <p class="desc">{t('onboarding.connect.hooks_help')}</p>
      </div>
      {#if hooks?.session_hooks_installed}<span class="ok"><Icon name="check" size={16} strokeWidth={2.2} />{t('pages.system.installed')}</span>{/if}
    </li>
  </ul>
  <p class="changes"><Icon name="info" size={16} /><span>{t('onboarding.connect.changes', { file })}</span></p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if connected}
    <p class="done" role="status"><Icon name="check" size={18} strokeWidth={2.2} />{result ? t('onboarding.connect.success') : t('onboarding.connect.already')}</p>
  {:else}
    <Button variant="primary" size="lg" icon="plug" {busy} onclick={connect}>{busy ? t('onboarding.connect.working') : t('onboarding.connect.action')}</Button>
  {/if}
</div>

<style>
  .parts {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  li {
    display: flex;
    gap: var(--sp-3);
    align-items: flex-start;
  }
  .glyph {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: var(--r-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }
  li > div {
    flex: 1;
  }
  .name {
    font-weight: var(--fw-semibold);
  }
  .desc {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 62ch;
  }
  .ok {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--good-ink);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    white-space: nowrap;
  }
  .changes {
    display: flex;
    gap: var(--sp-2);
    margin: var(--sp-5) 0;
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .changes :global(svg) {
    margin-top: 2px;
    color: var(--ink-3);
  }
  .error {
    margin-bottom: var(--sp-3);
    color: var(--critical-ink);
  }
  .done {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--good-ink);
    font-weight: var(--fw-semibold);
  }
</style>
