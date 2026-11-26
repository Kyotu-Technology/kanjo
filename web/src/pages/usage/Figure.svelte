<script lang="ts">
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';

  interface Props {
    label: string;
    value: string;
    help?: string;
    meta?: string;
  }
  let { label, value, help, meta }: Props = $props();
</script>

<div class="fig">
  <span class="label">{label}{#if help}<InlineHelp text={help} {label} />{/if}</span>
  <span class="value">{value}</span>
  {#if meta}<span class="meta">{meta}</span>{/if}
</div>

<style>
  .fig {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .label {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .value {
    margin: 2px 0;
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
    letter-spacing: var(--tracking-tight);
    font-variant-numeric: tabular-nums;
    line-height: var(--lh-tight);
  }
  .meta {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
</style>
