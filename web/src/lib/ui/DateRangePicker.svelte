<script lang="ts">
  import { fmt, i18n, t } from '../i18n/index.svelte';
  import { customRange, PRESETS, rangeFor, toDateInput, type DateRange } from '../range';
  import Icon from './Icon.svelte';

  interface Props {
    value: DateRange;
    onchange?: (r: DateRange) => void;
  }
  let { value = $bindable(), onchange }: Props = $props();
  const uid = $props.id();
  let open = $state(false);
  let trigger: HTMLButtonElement | undefined = $state();
  let menu: HTMLDivElement | undefined = $state();
  let fromInput = $state('');
  let toInput = $state('');
  const custom = $derived(customRange(fromInput, toInput));

  const label = $derived(
    value.preset === 'custom' ? `${fmt.date(value.from, 'short')} - ${fmt.date(value.to - 1, 'short')}` : t(`range.${value.preset}`),
  );

  function choose(r: DateRange) {
    value = r;
    onchange?.(r);
    open = false;
    trigger?.focus();
  }

  function items(): HTMLElement[] {
    return [...(menu?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? [])];
  }

  $effect(() => {
    if (!open) return;
    fromInput = toDateInput(value.from);
    toInput = toDateInput(value.to - 1);
    queueMicrotask(() => (items().find((el) => el.getAttribute('aria-checked') === 'true') ?? items()[0])?.focus());
    const onDown = (e: PointerEvent) => {
      if (!menu?.contains(e.target as Node) && !trigger?.contains(e.target as Node)) open = false;
    };
    window.addEventListener('pointerdown', onDown);
    return () => window.removeEventListener('pointerdown', onDown);
  });

  function onMenuKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      open = false;
      trigger?.focus();
      return;
    }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const list = items();
    const i = list.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    e.preventDefault();
    list[(i + (e.key === 'ArrowDown' ? 1 : -1) + list.length) % list.length].focus();
  }
</script>

<div class="drp">
  <button
    bind:this={trigger}
    type="button"
    class="trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls="drp-{uid}"
    onclick={() => (open = !open)}
    onkeydown={(e) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        open = true;
      }
    }}
  >
    <Icon name="calendar" size={16} />
    <span class="sr-only">{t('range.label')}:</span>
    <span>{label}</span>
    <Icon name="chevron_down" size={16} />
  </button>
  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div bind:this={menu} id="drp-{uid}" class="menu" role="menu" aria-label={t('range.label')} onkeydown={onMenuKey}>
      {#each PRESETS as p (p)}
        <button type="button" role="menuitemradio" aria-checked={value.preset === p} tabindex="-1" onclick={() => choose(rangeFor(p, i18n.now))}>
          <span class="check">{#if value.preset === p}<Icon name="check" size={16} strokeWidth={2.4} />{/if}</span>
          {t(`range.${p}`)}
        </button>
      {/each}
      <form
        class="custom"
        onsubmit={(e) => {
          e.preventDefault();
          if (custom) choose(custom);
        }}
      >
        <p class="custom-title">{t('range.custom')}</p>
        <div class="inputs">
          <label>
            <span>{t('range.from')}</span>
            <input type="date" bind:value={fromInput} max={toInput} />
          </label>
          <label>
            <span>{t('range.to')}</span>
            <input type="date" bind:value={toInput} min={fromInput} />
          </label>
        </div>
        <button type="submit" class="apply" disabled={!custom}>{t('range.apply')}</button>
      </form>
    </div>
  {/if}
</div>

<style>
  .drp {
    position: relative;
    display: inline-block;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 36px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--ink);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .trigger :global(svg:first-child) {
    color: var(--ink-3);
  }
  .menu {
    position: absolute;
    z-index: 50;
    top: calc(100% + 6px);
    left: 0;
    width: 260px;
    padding: var(--sp-1);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
    box-shadow: var(--shadow-overlay);
  }
  [role='menuitemradio'] {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    width: 100%;
    min-height: 34px;
    padding: 0 var(--sp-2);
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--ink);
    font-size: var(--fs-sm);
    text-align: left;
  }
  [role='menuitemradio']:hover,
  [role='menuitemradio']:focus-visible {
    background: var(--surface-2);
  }
  [aria-checked='true'] {
    font-weight: var(--fw-semibold);
  }
  .check {
    width: 16px;
    display: inline-grid;
    color: var(--accent);
  }
  .custom {
    margin-top: var(--sp-1);
    padding: var(--sp-3) var(--sp-2) var(--sp-2);
    border-top: 1px solid var(--line);
  }
  .custom-title {
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
    color: var(--ink-2);
  }
  .inputs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--sp-2);
    margin-top: var(--sp-2);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  input {
    min-height: 32px;
    padding: 0 var(--sp-1);
    border: 1px solid var(--control-border);
    border-radius: var(--r-xs);
    background: var(--surface);
    font-size: var(--fs-xs);
    min-width: 0;
  }
  .apply {
    margin-top: var(--sp-2);
    width: 100%;
    min-height: 32px;
    border: 1px solid var(--accent);
    border-radius: var(--r-sm);
    background: var(--accent);
    color: var(--accent-ink);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .apply:disabled {
    opacity: 0.5;
  }
</style>
