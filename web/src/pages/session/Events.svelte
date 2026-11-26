<script lang="ts">
  import type { SessionEvent } from '../../lib/api/contract';
  import { fmt, t, tk, tn } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';

  interface Props {
    events: SessionEvent[];
  }
  let { events }: Props = $props();

  type Item = { key: string; ts: number; title: string; sub: string | null; quiet: boolean };
  const lookup = (prefix: string, code: string) => {
    const s = tk(`${prefix}.${code}`);
    return s.startsWith('pages.') ? code : s;
  };
  const items = $derived.by(() => {
    const out: Item[] = [];
    let stops: SessionEvent[] = [];
    const flush = () => {
      if (!stops.length) return;
      const first = stops[0].ts;
      const last = stops[stops.length - 1].ts;
      out.push({
        key: `stop-${first}`,
        ts: first,
        title: stops.length === 1 ? t('pages.session.events.name.Stop') : tn('pages.session.events.answers', stops.length),
        sub: stops.length === 1 ? null : `${fmt.time(first)} - ${fmt.time(last)}`,
        quiet: true,
      });
      stops = [];
    };
    for (const e of [...events].sort((a, b) => a.ts - b.ts)) {
      if (e.event === 'Stop') {
        stops.push(e);
        continue;
      }
      flush();
      out.push({
        key: `${e.event}-${e.ts}`,
        ts: e.ts,
        title: lookup('pages.session.events.name', e.event),
        sub: e.source ? lookup('pages.session.events.source', e.source) : null,
        quiet: false,
      });
    }
    flush();
    return out;
  });
</script>

<Card title={t('pages.session.events.title')}>
  {#snippet actions()}<InlineHelp text={t('pages.session.events.help')} label={t('pages.session.events.title')} />{/snippet}
  {#if !items.length}
    <p class="sub">{t('pages.session.events.none')}</p>
  {:else}
    <ol class="tl">
      {#each items as it (it.key)}
        <li class:quiet={it.quiet}>
          <span class="time">{fmt.time(it.ts)}</span>
          <span class="dot" aria-hidden="true"></span>
          <span class="what">
            {it.title}
            {#if it.sub}<span class="sub">{it.sub}</span>{/if}
          </span>
        </li>
      {/each}
    </ol>
  {/if}
</Card>

<style>
  .tl {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  .tl::before {
    content: '';
    position: absolute;
    left: calc(3.4rem + 4px);
    top: 8px;
    bottom: 8px;
    width: 1px;
    background: var(--line-strong);
  }
  li {
    position: relative;
    display: grid;
    grid-template-columns: 3.4rem 9px minmax(0, 1fr);
    gap: var(--sp-3);
    align-items: start;
    font-size: var(--fs-sm);
  }
  .time {
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 9px;
    height: 9px;
    margin-top: 5px;
    border-radius: 50%;
    background: var(--surface);
    border: 2px solid var(--accent);
  }
  .quiet .dot {
    border-color: var(--line-strong);
  }
  .what {
    font-weight: var(--fw-medium);
  }
  .quiet .what {
    font-weight: var(--fw-regular);
    color: var(--ink-2);
  }
  .sub {
    display: block;
    color: var(--ink-3);
    font-size: var(--fs-xs);
    font-weight: var(--fw-regular);
  }
</style>
