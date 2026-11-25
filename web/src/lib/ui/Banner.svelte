<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    tone?: 'info' | 'warning' | 'progress';
    icon?: IconName;
    title: string;
    body?: string;
    /** 0..1 for a progress banner. */
    progress?: number;
    action?: Snippet;
    children?: Snippet;
  }
  let { tone = 'info', icon, title, body, progress, action, children }: Props = $props();
  const glyph = $derived<IconName>(icon ?? (tone === 'warning' ? 'warning' : tone === 'progress' ? 'refresh' : 'info'));
</script>

<div class="banner {tone}" role={tone === 'warning' ? 'alert' : 'status'}>
  <span class="glyph" class:spin={tone === 'progress'}><Icon name={glyph} size={20} /></span>
  <div class="text">
    <p class="title">{title}</p>
    {#if body}<p class="body">{body}</p>{/if}
    {#if children}{@render children()}{/if}
    {#if progress !== undefined}
      <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(progress * 100)} aria-label={title}>
        <span style="width:{Math.max(2, progress * 100)}%"></span>
      </div>
    {/if}
  </div>
  {#if action}<div class="action">{@render action()}</div>{/if}
</div>

<style>
  .banner {
    display: flex;
    gap: var(--sp-3);
    align-items: flex-start;
    padding: var(--sp-4) var(--sp-5);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .glyph {
    color: var(--accent);
    margin-top: 1px;
  }
  .warning {
    background: var(--warning-soft);
    border-color: color-mix(in srgb, var(--warning) 45%, transparent);
  }
  .warning .glyph {
    color: var(--warning-ink);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: var(--fw-semibold);
  }
  .body {
    margin-top: 2px;
    color: var(--ink-2);
    max-width: 75ch;
  }
  .action {
    flex: none;
    align-self: center;
  }
  .bar {
    margin-top: var(--sp-3);
    height: 6px;
    border-radius: var(--r-pill);
    background: var(--meter-track);
    overflow: hidden;
    max-width: 520px;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--meter-fill);
    transition: width var(--dur) var(--ease);
  }
  .spin :global(svg) {
    animation: spin 2.4s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (max-width: 640px) {
    .banner {
      flex-wrap: wrap;
    }
    .action {
      flex-basis: 100%;
      padding-left: calc(20px + var(--sp-3));
    }
  }
</style>
