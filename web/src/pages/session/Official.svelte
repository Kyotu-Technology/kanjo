<script lang="ts">
  import type { SessionDetail } from '../../lib/api/contract';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import Rich, { strong } from '../../lib/ui/Rich.svelte';

  interface Props {
    detail: SessionDetail;
  }
  let { detail }: Props = $props();
  const s = $derived(detail.status);
  const kanjo = $derived(detail.summary.usage.cost_usd);
  const official = $derived(s?.cost_usd ?? null);
  const diff = $derived(official === null ? null : official - kanjo);
  const signed = (v: number) => `${v > 0 ? '+' : v < 0 ? '-' : ''}${fmt.money(Math.abs(v))}`;
</script>

<Card title={t('pages.session.official.title')}>
  {#if s && official !== null}
    <div class="pair">
      <div class="side">
        <span class="k">{t('pages.session.official.claude')}</span>
        <Money usd={official} confidence="official" size="lg" />
      </div>
      <div class="side">
        <span class="k">{t('pages.session.official.kanjo')}</span>
        <Money usd={kanjo} confidence="lower_bound" unpriced={detail.summary.usage.unpriced_requests} size="lg" />
      </div>
    </div>
    {#if diff !== null}
      <p class="diff">
        <span>{t('pages.session.official.diff')}</span>
        <strong>{signed(diff)}</strong>
        {#if kanjo > 0}<span class="pct">{fmt.pct((Math.abs(diff) / kanjo) * 100, 1)}</span>{/if}
      </p>
    {/if}
    <h3>{t('pages.session.official.why')}</h3>
    <p class="why">{t('pages.session.official.why_body')}</p>
    <dl>
      <div><dt>{t('pages.session.official.snapshot')}</dt><dd>{fmt.ago(s.ts)}</dd></div>
      {#if s.api_duration_ms !== null}<div><dt>{t('pages.session.official.api_time')}</dt><dd>{fmt.duration(s.api_duration_ms)}</dd></div>{/if}
      {#if s.lines_added !== null || s.lines_removed !== null}
        <div><dt>{t('pages.session.official.lines')}</dt><dd>{t('pages.session.official.lines_value', { added: fmt.number(s.lines_added ?? 0), removed: fmt.number(s.lines_removed ?? 0) })}</dd></div>
      {/if}
    </dl>
  {:else}
    <p class="none-title">{t('pages.session.official.none_title')}</p>
    <p class="why"><Rich text={t('pages.session.official.none_body', { money: strong(fmt.money(kanjo)) })} /></p>
    <p class="why muted">{t('pages.session.official.why_body')}</p>
  {/if}
</Card>

<style>
  .pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: var(--sp-4);
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .k {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .diff {
    display: flex;
    align-items: baseline;
    gap: var(--sp-2);
    margin-top: var(--sp-4);
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .diff strong {
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }
  .pct {
    color: var(--ink-3);
  }
  h3 {
    margin-top: var(--sp-4);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .why {
    margin-top: var(--sp-1);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    max-width: 68ch;
  }
  .muted {
    margin-top: var(--sp-3);
    color: var(--ink-3);
  }
  .none-title {
    font-weight: var(--fw-semibold);
  }
  dl {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    margin-top: var(--sp-4);
    padding-top: var(--sp-3);
    border-top: 1px solid var(--line);
    font-size: var(--fs-sm);
  }
  dl div {
    display: flex;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  dt {
    color: var(--ink-3);
  }
  dd {
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
</style>
