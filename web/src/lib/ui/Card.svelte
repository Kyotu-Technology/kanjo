<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title?: string;
    subtitle?: string;
    actions?: Snippet;
    children: Snippet;
    footer?: Snippet;
    /** Dims the body while newer data loads, without blanking it. */
    refreshing?: boolean;
    flush?: boolean;
    class?: string;
    id?: string;
  }

  let { title, subtitle, actions, children, footer, refreshing = false, flush = false, class: cls = '', id }: Props = $props();
  const uid = $props.id();
  const headingId = $derived(id ?? `card-${uid}`);
</script>

<section class="card {cls}" class:flush aria-labelledby={title ? headingId : undefined} aria-busy={refreshing || undefined}>
  {#if title || actions}
    <header class="head">
      <div class="titles">
        {#if title}<h2 id={headingId}>{title}</h2>{/if}
        {#if subtitle}<p class="subtitle">{subtitle}</p>{/if}
      </div>
      {#if actions}<div class="actions">{@render actions()}</div>{/if}
    </header>
  {/if}
  <div class="body" class:refreshing>
    {@render children()}
  </div>
  {#if footer}<footer class="foot">{@render footer()}</footer>{/if}
</section>

<style>
  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--sp-3);
    padding: var(--sp-5) var(--sp-5) 0;
  }
  h2 {
    font-size: var(--fs-md);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.01em;
  }
  .subtitle {
    margin-top: 2px;
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .actions {
    display: flex;
    gap: var(--sp-1);
    flex: none;
  }
  .body {
    padding: var(--sp-4) var(--sp-5) var(--sp-5);
    flex: 1;
    min-width: 0;
    transition: opacity var(--dur) var(--ease);
  }
  .flush .body {
    padding: var(--sp-3) 0 0;
  }
  .refreshing {
    opacity: 0.6;
  }
  .foot {
    border-top: 1px solid var(--line);
    padding: var(--sp-3) var(--sp-5);
    font-size: var(--fs-sm);
  }
</style>
