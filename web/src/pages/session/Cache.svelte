<script lang="ts">
  import type { SessionDetail } from '../../lib/api/contract';
  import { fmt, t, tk, tn } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import InlineHelp from '../../lib/ui/InlineHelp.svelte';

  interface Props {
    detail: SessionDetail;
  }
  let { detail }: Props = $props();
  const tok = $derived(detail.summary.usage.tokens);
  const parts = $derived([
    { key: 'read', label: t('pages.session.cache.read'), value: tok.cache_read, color: 'var(--seq-6)' },
    { key: 'written', label: t('pages.session.cache.written'), value: tok.cache_write_5m + tok.cache_write_1h, color: 'var(--seq-4)' },
    { key: 'fresh', label: t('pages.session.cache.fresh'), value: tok.input, color: 'var(--seq-2)' },
  ]);
  const sum = $derived(parts.reduce((a, p) => a + p.value, 0));
  const ratio = $derived(detail.summary.cache_hit_ratio);
  const c = $derived(detail.status?.cache ?? null);
  const misses = $derived([...detail.cache_misses].sort((a, b) => b.ts - a.ts));
  const causeText = (code: string) => {
    const s = tk(`pages.session.cache.cause.${code}`);
    return s.startsWith('pages.') ? code : s;
  };
  const kindText = (kind: string) => {
    const s = tk(`pages.session.cache.change.${kind}`);
    return s.startsWith('pages.') ? kind : s;
  };
</script>

<Card title={t('pages.session.cache.title')}>
  {#snippet actions()}<InlineHelp text={t('pages.session.cache.help')} label={t('pages.session.cache.title')} />{/snippet}
  <div class="top">
    <div>
      <p class="big">{ratio === null ? t('common.unknown') : fmt.ratio(ratio)}</p>
      <p class="sub">{t('pages.session.cache.reused')}</p>
    </div>
    {#if c && c.warm !== null}
      <p class="temp" class:cold={!c.warm}>
        <Icon name={c.warm ? 'clock' : 'info'} size={16} />
        <span>
          {c.warm && c.expires_at ? t('pages.session.cache.warm', { time: fmt.time(c.expires_at) }) : t('pages.session.cache.cold')}
          {#if !c.warm && c.recache_tokens_if_cold}<span class="sub block">{t('pages.session.cache.rebuild', { tokens: fmt.compact(c.recache_tokens_if_cold) })}</span>{/if}
        </span>
      </p>
    {/if}
  </div>
  {#if sum > 0}
    <div class="bar" aria-hidden="true">
      {#each parts as p (p.key)}{#if p.value > 0}<span style="flex:{p.value};background:{p.color}"></span>{/if}{/each}
    </div>
    <ul class="legend">
      {#each parts as p (p.key)}
        <li><span class="key" style="background:{p.color}"></span>{p.label}<strong>{fmt.compact(p.value)}</strong><span class="sub">{fmt.ratio(p.value / sum)}</span></li>
      {/each}
    </ul>
  {/if}

  <h3>{t('pages.session.cache.misses')}{#if misses.length}<span class="count">{tn('pages.session.cache.misses_count', misses.length)}</span>{/if}</h3>
  {#if !misses.length}
    <p class="sub">{t('pages.session.cache.misses_none')}</p>
  {:else}
    <ol class="misses">
      {#each misses as m (m.ts)}
        <li>
          <span class="when">{fmt.time(m.ts)}</span>
          <div>
            <p class="cause">{m.causes.map(causeText).join('; ')}</p>
            {#if m.tools_added || m.tools_removed}<p class="sub">{t('pages.session.cache.tools_delta', { added: m.tools_added ?? 0, removed: m.tools_removed ?? 0 })}</p>{/if}
            {#if m.system_char_delta}<p class="sub">{t(m.system_char_delta > 0 ? 'pages.session.cache.system_delta' : 'pages.session.cache.system_delta_down', { chars: fmt.number(Math.abs(m.system_char_delta)) })}</p>{/if}
            {#if m.correlated_changes.length}
              <p class="before">{t('pages.session.cache.before')}</p>
              <ul class="changes">
                {#each m.correlated_changes as ch (ch.ts + ch.target)}
                  <li>
                    <Icon name="settings" size={14} />
                    {t('pages.session.cache.change_line', { what: kindText(ch.kind), target: ch.target, duration: fmt.duration(Math.max(60_000, m.ts - ch.ts)) })}
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  {/if}
</Card>

<style>
  .top {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--sp-3) var(--sp-5);
  }
  .big {
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
    letter-spacing: var(--tracking-tight);
    font-variant-numeric: tabular-nums;
  }
  .sub {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .block {
    display: block;
    font-size: var(--fs-xs);
  }
  .temp {
    display: flex;
    gap: var(--sp-2);
    align-items: flex-start;
    max-width: 34ch;
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    font-size: var(--fs-sm);
  }
  .temp :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--good-ink);
  }
  .temp.cold :global(svg) {
    color: var(--ink-3);
  }
  .bar {
    display: flex;
    gap: 2px;
    height: 10px;
    margin-top: var(--sp-4);
    border-radius: var(--r-pill);
    overflow: hidden;
  }
  .bar span {
    min-width: 2px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2) var(--sp-5);
    margin-top: var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .legend li {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
  }
  .legend strong {
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }
  .key {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    align-self: center;
  }
  h3 {
    display: flex;
    align-items: baseline;
    gap: var(--sp-2);
    margin: var(--sp-5) 0 var(--sp-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .count {
    color: var(--ink-3);
    font-weight: var(--fw-regular);
  }
  .misses {
    display: flex;
    flex-direction: column;
  }
  .misses > li {
    display: grid;
    grid-template-columns: 3.4rem minmax(0, 1fr);
    gap: var(--sp-3);
    padding: var(--sp-3) 0;
    border-top: 1px solid var(--line);
  }
  .when {
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  .cause {
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .before {
    margin-top: var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .changes li {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .changes :global(svg) {
    flex: none;
    color: var(--ink-3);
  }
</style>
