<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '../i18n/index.svelte';
  import Card from '../ui/Card.svelte';
  import Button from '../ui/Button.svelte';
  import Legend from './Legend.svelte';

  interface Props {
    title: string;
    subtitle?: string;
    legend?: { label: string; color: string }[];
    legendKind?: 'rect' | 'line';
    /** Extra controls beside the table toggle. */
    actions?: Snippet;
    /** Text above the chart that states the finding, e.g. the peak. */
    note?: Snippet;
    chart: Snippet;
    table: Snippet;
    refreshing?: boolean;
    class?: string;
  }
  let { title, subtitle, legend = [], legendKind = 'rect', actions: extraActions, note, chart, table, refreshing = false, class: cls = '' }: Props = $props();
  let asTable = $state(false);
</script>

<Card {title} {subtitle} {refreshing} class={cls}>
  {#snippet actions()}
    {#if extraActions}{@render extraActions()}{/if}
    <Button
      variant="ghost"
      size="sm"
      icon={asTable ? 'chart' : 'table'}
      iconOnly
      aria-pressed={asTable}
      title={asTable ? t('common.show_chart') : t('common.show_table')}
      onclick={() => (asTable = !asTable)}>{asTable ? t('common.show_chart') : t('common.show_table')}</Button
    >
  {/snippet}
  {#if note}<div class="note">{@render note()}</div>{/if}
  {#if !asTable && legend.length > 1}<div class="legend"><Legend items={legend} kind={legendKind} /></div>{/if}
  {#if asTable}{@render table()}{:else}{@render chart()}{/if}
</Card>

<style>
  .legend {
    margin-bottom: var(--sp-3);
  }
  .note {
    margin-bottom: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
</style>
