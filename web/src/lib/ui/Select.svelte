<script lang="ts" generics="V extends string | number">
  import Icon from './Icon.svelte';

  interface Props {
    label: string;
    value: V;
    options: { value: V; label: string }[];
    hideLabel?: boolean;
    onchange?: (v: V) => void;
    size?: 'sm' | 'md';
  }
  let { label, value = $bindable(), options, hideLabel = false, onchange, size = 'md' }: Props = $props();
  const uid = $props.id();
  const id = `sel-${uid}`;
</script>

<div class="field {size}">
  <label for={id} class:sr-only={hideLabel}>{label}</label>
  <span class="wrap">
    <select
      {id}
      value={String(value)}
      onchange={(e) => {
        const raw = (e.currentTarget as HTMLSelectElement).value;
        const found = options.find((o) => String(o.value) === raw);
        if (found) {
          value = found.value;
          onchange?.(found.value);
        }
      }}
    >
      {#each options as o (o.value)}
        <option value={String(o.value)}>{o.label}</option>
      {/each}
    </select>
    <Icon name="chevron_down" size={16} class="chev" />
  </span>
</div>

<style>
  .field {
    display: inline-flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  label {
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    color: var(--ink-2);
  }
  .wrap {
    position: relative;
    display: inline-flex;
  }
  select {
    appearance: none;
    min-height: 36px;
    width: 100%;
    padding: 0 34px 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--ink);
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .sm select {
    min-height: 30px;
    font-size: var(--fs-xs);
  }
  .wrap :global(.chev) {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
    color: var(--ink-3);
  }
</style>
