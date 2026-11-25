<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    text: string;
    label?: string;
  }
  let { text, label }: Props = $props();
  const uid = $props.id();
  const id = `help-${uid}`;
  let open = $state(false);
  let btn: HTMLButtonElement | undefined = $state();
  let panel: HTMLDivElement | undefined = $state();
  let pos = $state({ top: 0, left: 0 });

  $effect(() => {
    if (!open || !btn || !panel) return;
    const r = btn.getBoundingClientRect();
    const w = panel.offsetWidth;
    const vw = document.documentElement.clientWidth;
    pos = { top: r.bottom + 8, left: Math.max(8, Math.min(vw - w - 8, r.left - 12)) };
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent) {
        if (e.key === 'Escape') {
          open = false;
          btn?.focus();
        }
        return;
      }
      if (!panel?.contains(e.target as Node) && !btn?.contains(e.target as Node)) open = false;
    };
    window.addEventListener('keydown', close);
    window.addEventListener('pointerdown', close);
    return () => {
      window.removeEventListener('keydown', close);
      window.removeEventListener('pointerdown', close);
    };
  });
</script>

<span class="help">
  <button bind:this={btn} type="button" class="q" aria-expanded={open} aria-controls={id} aria-label={label ?? t('common.help')} onclick={() => (open = !open)}>
    <Icon name="help" size={16} />
  </button>
  <div bind:this={panel} {id} class="panel" class:open style="top:{pos.top}px;left:{pos.left}px" role="note">{text}</div>
</span>

<style>
  .help {
    display: inline-flex;
    vertical-align: middle;
  }
  .q {
    display: inline-grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--ink-3);
  }
  .q:hover,
  .q[aria-expanded='true'] {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .panel {
    position: fixed;
    z-index: 60;
    width: max-content;
    max-width: min(320px, calc(100vw - 16px));
    padding: var(--sp-3) var(--sp-4);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-regular);
    line-height: var(--lh-body);
    box-shadow: var(--shadow-overlay);
    display: none;
  }
  .panel.open {
    display: block;
  }
</style>
