<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Tooltip text; plain text only. */
    text: string;
    /** Accessible name of the trigger when its content is not text. */
    label?: string;
    children: Snippet;
    class?: string;
  }

  let { text, label, children, class: cls = '' }: Props = $props();
  const uid = $props.id();
  const id = `tip-${uid}`;
  let open = $state(false);
  let trigger: HTMLButtonElement | undefined = $state();
  let tip: HTMLDivElement | undefined = $state();
  let pos = $state({ top: 0, left: 0, above: false });

  function place() {
    if (!trigger || !tip) return;
    const r = trigger.getBoundingClientRect();
    const w = tip.offsetWidth;
    const h = tip.offsetHeight;
    const vw = document.documentElement.clientWidth;
    const above = r.bottom + h + 12 > window.innerHeight && r.top > h + 12;
    pos = {
      top: above ? r.top - h - 8 : r.bottom + 8,
      left: Math.max(8, Math.min(vw - w - 8, r.left + r.width / 2 - w / 2)),
      above,
    };
  }

  $effect(() => {
    if (!open) return;
    place();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') open = false;
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('scroll', place, true);
    };
  });
</script>

<button
  bind:this={trigger}
  type="button"
  class="tip-trigger {cls}"
  aria-describedby={id}
  aria-label={label}
  onpointerenter={() => (open = true)}
  onpointerleave={() => (open = false)}
  onfocus={() => (open = true)}
  onblur={() => (open = false)}
  onclick={() => (open = true)}
>
  {@render children()}
</button>
<div bind:this={tip} {id} role="tooltip" class="tip" class:open style="top:{pos.top}px;left:{pos.left}px">{text}</div>

<style>
  .tip-trigger {
    display: inline-flex;
    align-items: center;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: help;
  }
  .tip {
    position: fixed;
    z-index: 60;
    max-width: min(280px, calc(100vw - 16px));
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--ink);
    color: var(--ink-inverse);
    font-size: var(--fs-sm);
    font-weight: var(--fw-regular);
    line-height: var(--lh-snug);
    letter-spacing: 0;
    text-align: left;
    white-space: normal;
    box-shadow: var(--shadow-overlay);
    pointer-events: none;
    visibility: hidden;
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .tip.open {
    visibility: visible;
    opacity: 1;
  }
</style>
