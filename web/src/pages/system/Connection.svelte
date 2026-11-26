<script lang="ts">
  import type { SystemInfo } from '../../lib/api/contract';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { api, cache, describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';
  import Icon, { type IconName } from '../../lib/ui/Icon.svelte';

  let { d, refreshing = false }: { d: SystemInfo; refreshing?: boolean } = $props();
  const h = $derived(d.hooks);
  const file = $derived(`${d.claude_dir.replace(/\/$/, '')}/settings.json`);
  const connected = $derived(h.statusline_installed && h.session_hooks_installed);
  const partly = $derived(h.statusline_installed || h.session_hooks_installed);
  let busy = $state<'connect' | 'disconnect' | 'service' | null>(null);
  let confirm = $state(false);

  async function run(kind: 'connect' | 'disconnect' | 'service') {
    busy = kind;
    confirm = false;
    try {
      if (kind === 'disconnect') await api().removeHooks();
      else await api().installHooks({ statusline: kind === 'connect', session_hooks: kind === 'connect', service: kind === 'service' });
      cache.invalidate(['system', 'limits', 'usage', 'sessions']);
      toasts.push('success', t(kind === 'connect' ? 'system.connect.done_connect' : kind === 'disconnect' ? 'system.connect.done_disconnect' : 'system.connect.done_service'));
    } catch (e) {
      toasts.push('error', describeError(e));
    } finally {
      busy = null;
    }
  }
</script>

{#snippet part(icon: IconName, name: string, on: boolean, text: string, onLabel = t('system.connect.connected'), offLabel = t('system.connect.not_connected'))}
  <div class="part">
    <span class="glyph" class:on aria-hidden="true"><Icon name={icon} size={18} /></span>
    <div class="ptext">
      <p class="name">
        {name}
        <span class="pill" class:on><Icon name={on ? 'check' : 'dot'} size={13} strokeWidth={2.4} />{on ? onLabel : offLabel}</span>
      </p>
      <p class="desc">{text}</p>
    </div>
  </div>
{/snippet}

<Card id="connection" title={t('system.connect.title')} subtitle={t('system.connect.subtitle')} {refreshing}>
  {#if d.demo}
  <p class="note"><Icon name="info" size={16} /><span>{t('system.connect.demo')}</span></p>
  {:else}
  <div class="parts">
    <div>
      {@render part('limits', t('system.connect.statusline'), h.statusline_installed, h.statusline_installed ? t('system.connect.statusline_on') : t('system.connect.statusline_off'))}
      {#if h.statusline_installed}
        <div class="sub">
          {#if h.last_statusline_at}<p class="meta"><Icon name="clock" size={14} />{t('system.connect.last_payload', { ago: fmt.ago(h.last_statusline_at) })}</p>{/if}
          <p class="meta-title">{t('system.connect.chain')}</p>
          {#if h.statusline_chain}
            <p class="desc">{t('system.connect.chain_on')}</p>
            <code class="cmd">{h.statusline_chain}</code>
          {:else}
            <p class="desc">{t('system.connect.chain_off')}</p>
          {/if}
        </div>
      {/if}
    </div>
    {@render part('sessions', t('system.connect.hooks'), h.session_hooks_installed, h.session_hooks_installed ? t('system.connect.hooks_on') : t('system.connect.hooks_off'))}
    <div>
      {@render part('refresh', t('system.connect.service'), h.service_installed, h.service_installed ? t('system.connect.service_on') : t('system.connect.service_off'), t('system.connect.installed'), t('system.connect.not_installed'))}
      <div class="sub">
        {#if h.service_installed}
          <p class="desc">{t('system.connect.service_remove')}</p>
        {:else}
          <Button size="sm" icon="plug" busy={busy === 'service'} disabled={busy !== null} onclick={() => run('service')}>{t('system.connect.service_install')}</Button>
        {/if}
      </div>
    </div>
  </div>
  <p class="note file"><Icon name="info" size={16} /><span>{t('system.connect.file', { file })}</span></p>
  <div class="form-foot">
    {#if !connected}
      <Button variant="primary" icon="plug" busy={busy === 'connect'} disabled={busy !== null} onclick={() => run('connect')}>{t('system.connect.connect')}</Button>
    {/if}
    {#if partly}
      <Button variant="secondary" busy={busy === 'disconnect'} disabled={busy !== null} onclick={() => (confirm = true)}>{t('system.connect.disconnect')}</Button>
    {/if}
  </div>
  {/if}
</Card>

<Dialog bind:open={confirm} title={t('system.connect.confirm_title')} size="sm">
  <p>{t('system.connect.confirm_body', { file })}</p>
  <p class="effect">{t('system.connect.confirm_effect')}</p>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (confirm = false)}>{t('common.cancel')}</Button>
    <Button variant="danger" onclick={() => run('disconnect')}>{t('system.connect.confirm_action')}</Button>
  {/snippet}
</Dialog>

<style>
  .parts {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .part {
    display: flex;
    gap: var(--sp-3);
  }
  .glyph {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: var(--r-sm);
    background: var(--surface-3);
    color: var(--ink-3);
  }
  .glyph.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .name {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
    font-weight: var(--fw-semibold);
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 8px;
    border-radius: var(--r-pill);
    background: var(--surface-3);
    color: var(--ink-2);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
  }
  .pill.on {
    background: var(--good-soft);
    color: var(--good-ink);
  }
  .desc {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 66ch;
  }
  .sub {
    margin: var(--sp-2) 0 0 calc(36px + var(--sp-3));
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--sp-1);
  }
  .meta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .meta-title {
    margin-top: var(--sp-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .cmd {
    display: block;
    max-width: 100%;
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    overflow-wrap: anywhere;
  }
  .file {
    margin-top: var(--sp-5);
  }
  .effect {
    margin-top: var(--sp-3);
    color: var(--ink-2);
  }
</style>
