<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    lede?: string;
    meta?: Snippet;
    /** The filter row: one row, above everything it scopes. */
    filters?: Snippet;
  }
  let { title, lede, meta, filters }: Props = $props();
</script>

<header class="ph">
  <div class="row">
    <h1>{title}</h1>
    {#if meta}<div class="meta">{@render meta()}</div>{/if}
  </div>
  {#if lede}<p class="lede">{lede}</p>{/if}
  {#if filters}<div class="filters">{@render filters()}</div>{/if}
</header>

<style>
  .ph {
    margin-bottom: var(--sp-6);
  }
  .row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--sp-3);
    flex-wrap: wrap;
  }
  h1 {
    font-size: var(--fs-xl);
    letter-spacing: -0.02em;
  }
  .meta {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .lede {
    margin-top: var(--sp-1);
    color: var(--ink-2);
    max-width: 70ch;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--sp-3);
    margin-top: var(--sp-4);
  }
</style>
