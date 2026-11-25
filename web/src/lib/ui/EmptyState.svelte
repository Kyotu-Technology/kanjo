<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    icon?: IconName;
    title: string;
    body?: string;
    action?: Snippet;
    compact?: boolean;
  }
  let { icon = 'info', title, body, action, compact = false }: Props = $props();
</script>

<div class="empty" class:compact>
  <span class="glyph"><Icon name={icon} size={compact ? 18 : 22} /></span>
  <div class="text">
    <p class="title">{title}</p>
    {#if body}<p class="body">{body}</p>{/if}
    {#if action}<div class="action">{@render action()}</div>{/if}
  </div>
</div>

<style>
  .empty {
    display: flex;
    gap: var(--sp-4);
    align-items: flex-start;
    padding: var(--sp-6) var(--sp-2);
    max-width: 60ch;
  }
  .compact {
    padding: var(--sp-3) 0;
    gap: var(--sp-3);
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--ink-3);
  }
  .compact .glyph {
    width: 32px;
    height: 32px;
  }
  .title {
    font-weight: var(--fw-semibold);
  }
  .body {
    margin-top: var(--sp-1);
    color: var(--ink-2);
  }
  .action {
    margin-top: var(--sp-3);
  }
</style>
