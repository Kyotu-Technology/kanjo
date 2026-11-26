<script lang="ts">
  interface Props {
    items: { label: string; color: string; title?: string }[];
    kind?: 'rect' | 'line';
  }
  let { items, kind = 'rect' }: Props = $props();
</script>

{#if items.length > 1}
  <ul class="legend">
    {#each items as item (item.label)}
      <li title={item.title}><span class="key {kind}" style="background:{item.color}"></span>{item.label}</li>
    {/each}
  </ul>
{/if}

<style>
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1) var(--sp-4);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .key {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }
  .key.line {
    width: 14px;
    height: 2px;
    border-radius: 1px;
  }
</style>
