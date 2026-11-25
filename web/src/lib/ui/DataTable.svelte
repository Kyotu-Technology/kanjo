<script lang="ts" module>
  import type { Snippet } from 'svelte';

  export interface Column<T> {
    key: string;
    label: string;
    align?: 'start' | 'end';
    /** Sort value; columns without one are not sortable. */
    value?: (row: T) => number | string;
    width?: string;
    /** Hidden below 720 px. */
    optional?: boolean;
    header?: Snippet;
  }
</script>

<script lang="ts" generics="T">
  import { t } from '../i18n/index.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    caption: string;
    hideCaption?: boolean;
    rows: T[];
    columns: Column<T>[];
    rowKey: (row: T) => string;
    cell: Snippet<[T, Column<T>]>;
    sortKey?: string;
    sortDir?: 'asc' | 'desc';
    empty?: string;
    dense?: boolean;
    onrowclick?: (row: T) => void;
  }

  let { caption, hideCaption = true, rows, columns, rowKey, cell, sortKey = $bindable(), sortDir = $bindable('desc'), empty, dense = false, onrowclick }: Props = $props();

  const sorted = $derived.by(() => {
    const col = columns.find((c) => c.key === sortKey);
    if (!col?.value) return rows;
    const get = col.value;
    const dir = sortDir === 'asc' ? 1 : -1;
    return [...rows].sort((a, b) => {
      const x = get(a);
      const y = get(b);
      return (typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y))) * dir;
    });
  });

  function toggle(col: Column<T>) {
    if (!col.value) return;
    if (sortKey === col.key) sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    else {
      sortKey = col.key;
      sortDir = rows.length && typeof col.value(rows[0]) === 'string' ? 'asc' : 'desc';
    }
  }

  const sortLabel = $derived(columns.find((c) => c.key === sortKey)?.label);
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="scroll" role="region" aria-label={caption} tabindex="0">
  <table class:dense>
    <caption class:sr-only={hideCaption}>
      {caption}{#if sortLabel}<span class="sr-only">, {t('chart.sorted_by', { column: sortLabel })}</span>{/if}
    </caption>
    <thead>
      <tr>
        {#each columns as col (col.key)}
          <th
            scope="col"
            class:end={col.align === 'end'}
            class:optional={col.optional}
            style:width={col.width}
            aria-sort={sortKey === col.key ? (sortDir === 'asc' ? 'ascending' : 'descending') : undefined}
          >
            {#if col.value}
              <button type="button" class="sort" class:active={sortKey === col.key} onclick={() => toggle(col)}>
                <span>{col.label}</span>
                {#if col.header}{@render col.header()}{/if}
                <Icon name="chevron_down" size={14} class="dir {sortKey === col.key && sortDir === 'asc' ? 'up' : ''}" />
              </button>
            {:else}
              <span class="plain">{col.label}{#if col.header}{@render col.header()}{/if}</span>
            {/if}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each sorted as row (rowKey(row))}
        <tr class:clickable={!!onrowclick} onclick={onrowclick ? () => onrowclick(row) : undefined}>
          {#each columns as col (col.key)}
            <td class:end={col.align === 'end'} class:optional={col.optional}>{@render cell(row, col)}</td>
          {/each}
        </tr>
      {:else}
        <tr><td class="empty" colspan={columns.length}>{empty ?? t('chart.no_data')}</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .scroll {
    overflow-x: auto;
    border-radius: var(--r-sm);
  }
  .scroll:focus-visible {
    outline-offset: -2px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  caption {
    text-align: left;
    font-weight: var(--fw-semibold);
    padding-bottom: var(--sp-2);
  }
  th {
    text-align: left;
    font-weight: var(--fw-medium);
    color: var(--ink-3);
    font-size: var(--fs-xs);
    padding: var(--sp-2) var(--sp-3);
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }
  td {
    padding: var(--sp-3);
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }
  .dense td {
    padding: var(--sp-2) var(--sp-3);
  }
  th:first-child,
  td:first-child {
    padding-left: var(--sp-5);
  }
  th:last-child,
  td:last-child {
    padding-right: var(--sp-5);
  }
  tbody tr:last-child td {
    border-bottom: 0;
  }
  .end {
    text-align: right;
  }
  .end .sort {
    flex-direction: row-reverse;
  }
  .sort,
  .plain {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .sort {
    border: 0;
    background: none;
    padding: 2px 0;
    color: inherit;
    font: inherit;
  }
  .sort:hover,
  .sort.active {
    color: var(--ink);
  }
  .sort :global(.dir) {
    opacity: 0;
    transition: transform var(--dur-fast) var(--ease);
  }
  .sort.active :global(.dir) {
    opacity: 1;
  }
  .sort :global(.dir.up) {
    transform: rotate(180deg);
  }
  tr.clickable {
    cursor: pointer;
  }
  tbody tr.clickable:hover td {
    background: var(--surface-2);
  }
  .empty {
    color: var(--ink-3);
    text-align: center;
    padding: var(--sp-6);
  }
  @media (max-width: 720px) {
    .optional {
      display: none;
    }
    th:first-child,
    td:first-child {
      padding-left: var(--sp-4);
    }
    th:last-child,
    td:last-child {
      padding-right: var(--sp-4);
    }
  }
</style>
