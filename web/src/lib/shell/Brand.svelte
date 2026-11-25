<script lang="ts">
  interface Props {
    compact?: boolean;
  }
  let { compact = false }: Props = $props();
</script>

<span class="brand" class:compact>
  <svg class="mark" viewBox="0 0 32 32" aria-hidden="true">
    <circle cx="16" cy="16" r="14.5" class="ring" />
    <path d="M9 20.5h14" class="rule" />
    <path d="M9 20.5V12M14.5 20.5V9.5M20 20.5v-6" class="bars" />
    <circle cx="23" cy="10" r="2" class="seal" />
  </svg>
  {#if !compact}<span class="word">Kanjo</span>{/if}
</span>

<style>
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    color: var(--ink);
  }
  .mark {
    width: 30px;
    height: 30px;
    flex: none;
  }
  .ring {
    fill: var(--accent);
  }
  .rule,
  .bars {
    stroke: var(--accent-ink);
    stroke-width: 2.2;
    stroke-linecap: round;
    fill: none;
  }
  .seal {
    fill: var(--accent-ink);
  }
  .word {
    font-size: 1.2rem;
    font-weight: var(--fw-bold);
    letter-spacing: -0.03em;
  }
</style>
