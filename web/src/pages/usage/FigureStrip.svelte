<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    children: Snippet;
    refreshing?: boolean;
  }
  let { label, children, refreshing = false }: Props = $props();
</script>

<section class="strip" class:refreshing aria-label={label}>
  {@render children()}
</section>

<style>
  .strip {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 160px), 1fr));
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
    overflow: hidden;
    transition: opacity var(--dur) var(--ease);
  }
  .refreshing {
    opacity: 0.6;
  }
  .strip > :global(*) {
    padding: var(--sp-4) var(--sp-5);
    min-width: 0;
    box-shadow: -1px 0 0 var(--line), 0 -1px 0 var(--line);
  }
</style>
