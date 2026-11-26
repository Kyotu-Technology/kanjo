<script lang="ts" module>
  const OPEN = String.fromCharCode(1);
  const CLOSE = String.fromCharCode(2);
  /** Marks a value inserted into a translated sentence for emphasis; render the sentence with <Rich>. */
  export function strong(value: string | number): string {
    return `${OPEN}${value}${CLOSE}`;
  }
</script>

<script lang="ts">
  interface Props {
    text: string;
  }
  let { text }: Props = $props();
  const parts = $derived(
    text.split(OPEN).flatMap((chunk, i) => {
      if (i === 0) return [{ s: chunk, b: false }];
      const [inner, rest = ''] = chunk.split(CLOSE);
      return [
        { s: inner, b: true },
        { s: rest, b: false },
      ];
    }),
  );
</script>

{#each parts as p, i (i)}{#if p.b}<strong>{p.s}</strong>{:else}{p.s}{/if}{/each}
