<script lang="ts">
  interface Row {
    key: string;
    label: string;
    value: string;
    /** 0..1 of the whole. */
    share: number;
    color?: string;
    sublabel?: string | null;
    href?: string;
  }
  interface Props {
    rows: Row[];
    label: string;
    shareFormat: (share: number) => string;
  }
  let { rows, label, shareFormat }: Props = $props();
  const max = $derived(Math.max(...rows.map((r) => r.share), 1e-9));
</script>

<ul class="share" aria-label={label}>
  {#each rows as r (r.key)}
    <li>
      <div class="line">
        <span class="name">
          {#if r.color}<span class="dot" style="background:{r.color}"></span>{/if}
          {#if r.href}<a href={r.href}>{r.label}</a>{:else}{r.label}{/if}
          {#if r.sublabel}<span class="sub">{r.sublabel}</span>{/if}
        </span>
        <span class="val"><span class="amount">{r.value}</span><span class="pct">{shareFormat(r.share)}</span></span>
      </div>
      <div class="track" aria-hidden="true">
        <span style="width:{(r.share / max) * 100}%;{r.color ? `background:${r.color}` : ''}"></span>
      </div>
    </li>
  {/each}
</ul>

<style>
  .share {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  .line {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--sp-3);
    font-size: var(--fs-sm);
  }
  .name {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: var(--fw-medium);
  }
  .name a {
    color: var(--ink);
    text-decoration: none;
  }
  .name a:hover {
    text-decoration: underline;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex: none;
    align-self: center;
  }
  .sub {
    color: var(--ink-3);
    font-weight: var(--fw-regular);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .val {
    display: inline-flex;
    gap: var(--sp-2);
    flex: none;
    font-variant-numeric: tabular-nums;
  }
  .amount {
    font-weight: var(--fw-semibold);
  }
  .pct {
    color: var(--ink-3);
    min-width: 3.2em;
    text-align: right;
  }
  .track {
    margin-top: 6px;
    height: 6px;
    border-radius: var(--r-pill);
    background: var(--surface-3);
  }
  .track span {
    display: block;
    height: 100%;
    min-width: 2px;
    border-radius: inherit;
    background: var(--series-1);
  }
</style>
