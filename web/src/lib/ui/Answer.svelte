<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Confidence } from '../api/contract';
  import ConfidenceBadge from './ConfidenceBadge.svelte';

  interface Props {
    /** The page's answer in one or two plain sentences; key figures in <strong>. */
    children: Snippet;
    /** Confidence of the money figures the sentence quotes. */
    confidence?: Confidence;
    refreshing?: boolean;
  }
  let { children, confidence, refreshing = false }: Props = $props();
</script>

<div class="answer" class:refreshing aria-live="polite">
  <p>{@render children()}</p>
  {#if confidence}<span class="badge"><ConfidenceBadge {confidence} /></span>{/if}
</div>

<style>
  .answer {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--sp-2) var(--sp-3);
    margin: calc(-1 * var(--sp-2)) 0 var(--sp-6);
    max-width: 78ch;
    transition: opacity var(--dur) var(--ease);
  }
  .refreshing {
    opacity: 0.6;
  }
  p {
    font-size: var(--fs-md);
    line-height: var(--lh-body);
    color: var(--ink-2);
    text-wrap: pretty;
  }
  p :global(strong) {
    color: var(--ink);
    font-weight: var(--fw-semibold);
    font-variant-numeric: tabular-nums;
  }
  .badge {
    display: inline-flex;
    transform: translateY(-1px);
  }
</style>
