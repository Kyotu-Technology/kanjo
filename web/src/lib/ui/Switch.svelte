<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    description?: string;
    disabled?: boolean;
    onchange?: (v: boolean) => void;
  }
  let { checked = $bindable(), label, description, disabled = false, onchange }: Props = $props();
  const uid = $props.id();
  const id = `sw-${uid}`;
</script>

<div class="row">
  <div class="text">
    <label for={id} id="{id}-l" class="label">{label}</label>
    {#if description}<p class="desc" id="{id}-d">{description}</p>{/if}
  </div>
  <button
    {id}
    type="button"
    role="switch"
    class="sw"
    aria-labelledby="{id}-l"
    aria-checked={checked}
    aria-describedby={description ? `${id}-d` : undefined}
    {disabled}
    onclick={() => {
      checked = !checked;
      onchange?.(checked);
    }}
  >
    <span class="knob"></span>
  </button>
</div>

<style>
  .row {
    display: flex;
    gap: var(--sp-5);
    align-items: flex-start;
    justify-content: space-between;
    padding: var(--sp-4) 0;
  }
  .row + :global(.row) {
    border-top: 1px solid var(--line);
  }
  .label {
    font-weight: var(--fw-semibold);
    cursor: pointer;
  }
  .desc {
    margin-top: 2px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 62ch;
  }
  .sw {
    position: relative;
    flex: none;
    width: 44px;
    height: 26px;
    margin-top: 2px;
    border-radius: var(--r-pill);
    border: 1px solid var(--control-border);
    background: var(--surface-3);
    padding: 0;
    transition: background var(--dur-fast) var(--ease);
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--surface);
    box-shadow: var(--shadow-raise);
    border: 1px solid var(--control-border);
    transition: transform var(--dur) var(--ease);
  }
  .sw[aria-checked='true'] {
    background: var(--accent);
    border-color: var(--accent);
  }
  .sw[aria-checked='true'] .knob {
    transform: translateX(18px);
    border-color: var(--accent);
  }
</style>
