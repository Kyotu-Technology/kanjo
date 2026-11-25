<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import { toasts } from '../stores/toast.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  const ICONS: Record<string, IconName> = { info: 'info', success: 'check', warning: 'warning', error: 'critical' };
</script>

<div class="toaster" role="region" aria-live="polite" aria-label={t('shell.notifications')}>
  {#each toasts.items as toast (toast.id)}
    <div class="toast {toast.kind}" role={toast.kind === 'error' ? 'alert' : 'status'}>
      <span class="glyph"><Icon name={ICONS[toast.kind]} /></span>
      <p>{toast.message}</p>
      {#if toast.action}
        <button
          type="button"
          class="act"
          onclick={() => {
            toast.action?.run();
            toasts.dismiss(toast.id);
          }}>{toast.action.label}</button
        >
      {/if}
      <button type="button" class="x" aria-label={t('common.close')} onclick={() => toasts.dismiss(toast.id)}><Icon name="x" size={16} /></button>
    </div>
  {/each}
</div>

<style>
  .toaster {
    position: fixed;
    z-index: 80;
    right: var(--sp-5);
    bottom: var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    width: min(400px, calc(100vw - 32px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: var(--sp-3);
    padding: var(--sp-3) var(--sp-3) var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    background: var(--ink);
    color: var(--ink-inverse);
    box-shadow: var(--shadow-overlay);
    font-size: var(--fs-sm);
    animation: in var(--dur) var(--ease);
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  p {
    flex: 1;
    padding-top: 1px;
  }
  .glyph {
    padding-top: 1px;
  }
  .success .glyph {
    color: var(--good);
  }
  .warning .glyph {
    color: var(--warning);
  }
  .error .glyph {
    color: #ff8f8f;
  }
  .act,
  .x {
    border: 0;
    background: none;
    color: inherit;
    padding: 2px 6px;
    border-radius: var(--r-xs);
  }
  .act {
    font-weight: var(--fw-semibold);
    text-decoration: underline;
  }
  .x {
    opacity: 0.75;
  }
  .x:hover {
    opacity: 1;
  }
  .toast :global(:focus-visible) {
    outline-color: var(--ink-inverse);
  }
  @media (max-width: 640px) {
    .toaster {
      left: var(--sp-4);
      right: var(--sp-4);
      bottom: var(--sp-4);
      width: auto;
    }
  }
</style>
