<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '../i18n/index.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    open: boolean;
    title: string;
    description?: string;
    /** Hides the visible heading (it stays the accessible name). */
    hideTitle?: boolean;
    size?: 'sm' | 'md' | 'lg';
    placement?: 'center' | 'top' | 'side';
    children: Snippet;
    actions?: Snippet;
    onclose?: () => void;
  }
  let { open = $bindable(), title, description, hideTitle = false, size = 'md', placement = 'center', children, actions, onclose }: Props = $props();
  const uid = $props.id();
  let el: HTMLDialogElement | undefined = $state();
  let opener: Element | null = null;

  $effect(() => {
    if (!el) return;
    if (open && !el.open) {
      opener = document.activeElement;
      el.showModal();
    } else if (!open && el.open) el.close();
  });

  function handleClose() {
    open = false;
    onclose?.();
    if (opener instanceof HTMLElement) opener.focus();
  }
</script>

<dialog
  bind:this={el}
  class="dialog {size} {placement}"
  aria-labelledby="dlg-{uid}"
  aria-describedby={description ? `dlg-d-${uid}` : undefined}
  onclose={handleClose}
  onclick={(e) => {
    if (e.target === el) open = false;
  }}
>
  {#if open}
    <div class="inner">
      <header class:sr-only={hideTitle}>
        <h2 id="dlg-{uid}">{title}</h2>
        {#if description}<p id="dlg-d-{uid}" class="desc">{description}</p>{/if}
      </header>
      <button type="button" class="x" aria-label={t('common.close')} onclick={() => (open = false)}><Icon name="x" /></button>
      <div class="content">{@render children()}</div>
      {#if actions}<footer>{@render actions()}</footer>{/if}
    </div>
  {/if}
</dialog>

<style>
  .dialog {
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--surface);
    color: var(--ink);
    box-shadow: var(--shadow-overlay);
    width: min(560px, calc(100vw - 24px));
    max-height: calc(100vh - 48px);
  }
  .sm {
    width: min(420px, calc(100vw - 24px));
  }
  .lg {
    width: min(760px, calc(100vw - 24px));
  }
  .top {
    margin-top: min(12vh, 120px);
  }
  .side {
    margin: 0;
    height: 100vh;
    max-height: 100vh;
    width: min(300px, 86vw);
    border-radius: 0 var(--r-lg) var(--r-lg) 0;
  }
  .dialog::backdrop {
    background: var(--scrim);
  }
  .dialog[open] {
    animation: rise var(--dur) var(--ease);
  }
  .side[open] {
    animation: slide var(--dur) var(--ease);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  @keyframes slide {
    from {
      transform: translateX(-16px);
      opacity: 0.6;
    }
  }
  .inner {
    position: relative;
    display: flex;
    flex-direction: column;
    max-height: inherit;
    height: 100%;
  }
  header {
    padding: var(--sp-5) var(--sp-7) 0 var(--sp-5);
  }
  h2 {
    font-size: var(--fs-lg);
  }
  .desc {
    margin-top: var(--sp-1);
    color: var(--ink-2);
  }
  .x {
    position: absolute;
    top: var(--sp-3);
    right: var(--sp-3);
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--ink-3);
  }
  .x:hover {
    background: var(--surface-3);
    color: var(--ink);
  }
  .content {
    padding: var(--sp-4) var(--sp-5) var(--sp-5);
    overflow: auto;
    flex: 1;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--sp-2);
    padding: var(--sp-3) var(--sp-5);
    border-top: 1px solid var(--line);
  }
</style>
