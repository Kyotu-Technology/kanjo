<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props extends Omit<HTMLButtonAttributes, 'children'> {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'sm' | 'md' | 'lg';
    icon?: IconName;
    iconOnly?: boolean;
    busy?: boolean;
    href?: string;
    children?: Snippet;
  }

  let { variant = 'secondary', size = 'md', icon, iconOnly = false, busy = false, href, children, class: cls = '', type = 'button', disabled, ...rest }: Props = $props();
</script>

{#if href}
  <a class="btn {variant} {size} {cls}" class:icon-only={iconOnly} {href} aria-label={rest['aria-label']} title={rest.title ?? undefined}>
    {#if icon}<Icon name={icon} size={size === 'sm' ? 16 : 18} />{/if}
    {#if children}<span class="label" class:sr-only={iconOnly}>{@render children()}</span>{/if}
  </a>
{:else}
  <button class="btn {variant} {size} {cls}" class:icon-only={iconOnly} class:busy {type} disabled={disabled || busy} aria-busy={busy || undefined} {...rest}>
    {#if busy}
      <span class="spinner" aria-hidden="true"></span>
    {:else if icon}
      <Icon name={icon} size={size === 'sm' ? 16 : 18} />
    {/if}
    {#if children}<span class="label" class:sr-only={iconOnly}>{@render children()}</span>{/if}
  </button>
{/if}

<style>
  .btn {
    --btn-bg: var(--surface);
    --btn-ink: var(--ink);
    --btn-border: var(--control-border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--sp-2);
    min-height: 36px;
    padding: 0 var(--sp-4);
    border: 1px solid var(--btn-border);
    border-radius: var(--r-sm);
    background: var(--btn-bg);
    color: var(--btn-ink);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    line-height: 1;
    text-decoration: none;
    white-space: nowrap;
    transition:
      background var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  .btn:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--btn-ink);
  }
  .sm {
    min-height: 30px;
    padding: 0 var(--sp-3);
    font-size: var(--fs-xs);
  }
  .lg {
    min-height: 44px;
    padding: 0 var(--sp-5);
    font-size: var(--fs-base);
  }
  .primary {
    --btn-bg: var(--accent);
    --btn-ink: var(--accent-ink);
    --btn-border: var(--accent);
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
  }
  .ghost {
    --btn-bg: transparent;
    --btn-border: transparent;
    --btn-ink: var(--ink-2);
  }
  .ghost:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--ink);
  }
  .danger {
    --btn-ink: var(--critical-ink);
  }
  .icon-only {
    padding: 0;
    width: 36px;
  }
  .icon-only.sm {
    width: 30px;
  }
  .btn:disabled {
    opacity: 0.55;
  }
  .spinner {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid currentColor;
    border-right-color: transparent;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
