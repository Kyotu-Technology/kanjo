<script lang="ts">
  interface Props {
    width?: string;
    height?: string;
    lines?: number;
    radius?: string;
  }
  let { width = '100%', height = '1em', lines = 1, radius = 'var(--r-xs)' }: Props = $props();
</script>

<span class="sk-wrap" aria-hidden="true">
  {#each Array.from({ length: lines }, (_, i) => i) as i (i)}
    <span class="sk" style="width:{lines > 1 && i === lines - 1 ? '60%' : width};height:{height};border-radius:{radius}"></span>
  {/each}
</span>

<style>
  .sk-wrap {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
    width: 100%;
  }
  .sk {
    display: block;
    background: linear-gradient(90deg, var(--surface-3) 0%, var(--surface-2) 50%, var(--surface-3) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.6s ease-in-out infinite;
  }
  @keyframes shimmer {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: -100% 0;
    }
  }
</style>
