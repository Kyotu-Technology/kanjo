<script lang="ts">
  import type { Confidence } from '../api/contract';
  import { t } from '../i18n/index.svelte';
  import Tip from './Tip.svelte';

  interface Props {
    confidence: Confidence;
    /** Seal only, for dense rows; the label stays available to assistive tech. */
    compact?: boolean;
  }

  let { confidence, compact = false }: Props = $props();
  const label = $derived(t(`confidence.${confidence}`));
  const help = $derived(t(`confidence.${confidence}_help`));
</script>

<Tip text={help} label={compact ? label : undefined} class="conf-tip">
  <span class="conf {confidence}" class:compact>
    <svg class="seal" viewBox="0 0 16 16" aria-hidden="true">
      {#if confidence === 'official'}
        <circle cx="8" cy="8" r="7" class="fill" />
        <path d="M4.8 8.3 7 10.4l4.2-4.6" class="mark-inv" />
      {:else if confidence === 'estimate'}
        <circle cx="8" cy="8" r="6.4" class="ring dashed" />
        <path d="M4.6 8.9c1-1.4 2-1.4 3.2-.5s2.3.9 3.6-.6" class="mark" />
      {:else}
        <circle cx="8" cy="8" r="6.4" class="ring" />
        <path d="M5.6 5.4 10.2 7.6 5.6 9.8M5.6 11.6h4.8" class="mark" />
      {/if}
    </svg>
    {#if !compact}<span class="text">{label}</span>{/if}
  </span>
</Tip>

<style>
  .conf {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 20px;
    padding: 0 7px 0 3px;
    border-radius: var(--r-pill);
    border: 1px solid var(--line-strong);
    background: var(--surface);
    color: var(--ink-2);
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
    line-height: 1;
    white-space: nowrap;
  }
  .conf.compact {
    padding: 0;
    border: 0;
    background: none;
    height: auto;
  }
  .estimate {
    border-style: dashed;
  }
  .seal {
    width: 14px;
    height: 14px;
    flex: none;
  }
  .fill {
    fill: var(--ink-2);
  }
  .ring {
    fill: none;
    stroke: var(--ink-2);
    stroke-width: 1.3;
  }
  .dashed {
    stroke-dasharray: 2.2 1.6;
  }
  .mark,
  .mark-inv {
    fill: none;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .mark {
    stroke: var(--ink-2);
  }
  .mark-inv {
    stroke: var(--surface);
  }
  :global(.conf-tip:hover) .conf,
  :global(.conf-tip:focus-visible) .conf {
    border-color: var(--ink-3);
    color: var(--ink);
  }
</style>
