<script lang="ts">
  import { goToSection } from './saver.svelte';

  interface Props {
    label: string;
    items: { id: string; label: string; badge?: string }[];
  }
  let { label, items }: Props = $props();
  let current = $state('');

  $effect(() => {
    const els = items.map((i) => document.getElementById(i.id)?.closest<HTMLElement>('.card') ?? null).filter((e): e is HTMLElement => !!e);
    const io = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        const h = visible[0]?.target.querySelector('h2');
        if (h?.id) current = h.id;
      },
      { rootMargin: '-80px 0px -60% 0px' },
    );
    for (const el of els) io.observe(el);
    return () => io.disconnect();
  });

  function go(id: string) {
    goToSection(id);
    current = id;
  }
</script>

<nav class="secnav" aria-label={label}>
  <p class="label" aria-hidden="true">{label}</p>
  <ul>
    {#each items as item (item.id)}
      <li>
        <button type="button" class:active={current === item.id} aria-current={current === item.id ? 'true' : undefined} onclick={() => go(item.id)}>
          <span>{item.label}</span>
          {#if item.badge}<span class="badge">{item.badge}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
</nav>

<style>
  .secnav {
    position: sticky;
    top: calc(var(--topbar-h) + var(--sp-5));
  }
  .label {
    padding: 0 var(--sp-3) var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border-left: 1px solid var(--line);
  }
  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-2);
    width: 100%;
    min-height: 32px;
    margin-left: -1px;
    padding: 0 var(--sp-3);
    border: 0;
    border-left: 2px solid transparent;
    background: none;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    text-align: left;
  }
  button:hover {
    color: var(--ink);
  }
  button.active {
    border-left-color: var(--accent);
    color: var(--ink);
    font-weight: var(--fw-semibold);
  }
  .badge {
    min-width: 18px;
    padding: 0 6px;
    border-radius: var(--r-pill);
    background: var(--warning-soft);
    color: var(--warning-ink);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
    line-height: 18px;
    text-align: center;
  }
</style>
