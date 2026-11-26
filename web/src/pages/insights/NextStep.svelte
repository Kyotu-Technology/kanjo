<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import Icon from '../../lib/ui/Icon.svelte';

  interface Props {
    text: string;
    children?: Snippet;
  }
  let { text, children }: Props = $props();
</script>

<div class="next">
  <Icon name="chevron_right" size={16} />
  <p><strong>{t('pages.insights.next')}:</strong> {text}</p>
  {#if children}<div class="act">{@render children()}</div>{/if}
</div>

<style>
  .next {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--sp-2) var(--sp-2);
    margin-top: var(--sp-4);
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .next :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--accent);
  }
  p {
    flex: 1 1 24ch;
    max-width: 70ch;
  }
  strong {
    color: var(--ink);
    font-weight: var(--fw-semibold);
  }
  .act {
    display: flex;
    gap: var(--sp-2);
  }
</style>
