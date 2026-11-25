<script lang="ts" generics="V extends string">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    items: { id: V; label: string }[];
    value: V;
    onchange?: (v: V) => void;
    children?: Snippet;
    size?: 'sm' | 'md';
  }
  let { label, items, value = $bindable(), onchange, children, size = 'md' }: Props = $props();
  const uid = $props.id();
  let list: HTMLDivElement | undefined = $state();

  function select(v: V, focus = false) {
    value = v;
    onchange?.(v);
    if (focus) queueMicrotask(() => list?.querySelector<HTMLButtonElement>(`[data-tab="${v}"]`)?.focus());
  }

  function onKey(e: KeyboardEvent) {
    const i = items.findIndex((x) => x.id === value);
    let next = -1;
    if (e.key === 'ArrowRight') next = (i + 1) % items.length;
    else if (e.key === 'ArrowLeft') next = (i - 1 + items.length) % items.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = items.length - 1;
    if (next < 0) return;
    e.preventDefault();
    select(items[next].id, true);
  }
</script>

<div class="tabs {size}">
  <div bind:this={list} class="list" role="tablist" aria-label={label} tabindex="-1" onkeydown={onKey}>
    {#each items as item (item.id)}
      <button
        type="button"
        role="tab"
        id="tab-{uid}-{item.id}"
        data-tab={item.id}
        aria-selected={item.id === value}
        aria-controls="panel-{uid}"
        tabindex={item.id === value ? 0 : -1}
        onclick={() => select(item.id)}
      >
        {item.label}
      </button>
    {/each}
  </div>
  {#if children}
    <div class="panel" role="tabpanel" id="panel-{uid}" aria-labelledby="tab-{uid}-{value}" tabindex="-1">
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .list {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border-radius: var(--r-sm);
    background: var(--surface-3);
    max-width: 100%;
    overflow-x: auto;
  }
  button {
    min-height: 30px;
    padding: 0 var(--sp-3);
    border: 0;
    border-radius: calc(var(--r-sm) - 2px);
    background: transparent;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    white-space: nowrap;
  }
  .sm button {
    min-height: 26px;
    font-size: var(--fs-xs);
  }
  button:hover {
    color: var(--ink);
  }
  button[aria-selected='true'] {
    background: var(--surface);
    color: var(--ink);
    box-shadow: var(--shadow-raise);
  }
  .panel {
    margin-top: var(--sp-4);
  }
  @media (max-width: 480px) {
    .list {
      display: flex;
      width: 100%;
    }
    button {
      flex: 1 1 auto;
      padding: 0 var(--sp-2);
    }
  }
  .panel:focus {
    outline: none;
  }
</style>
