<script lang="ts">
  import type { Settings } from '../../lib/api/contract';
  import { fmt, i18n, t, tk } from '../../lib/i18n/index.svelte';
  import Icon, { type IconName } from '../../lib/ui/Icon.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import { listPrice } from './plans';
  import { around, goToSection } from './saver.svelte';

  let { s }: { s: Settings } = $props();

  const list = (items: string[]) => {
    try {
      return new Intl.ListFormat(i18n.locale, { type: 'conjunction' }).format(items);
    } catch {
      return items.join(', ');
    }
  };

  const plan = $derived.by(() => {
    const name = tk(`plan.${s.plan.kind}`);
    if (s.plan.kind === 'unknown') return { text: [t('settings.summary.plan_none'), ''] as [string, string], usd: null };
    const paid = s.plan.monthly_usd;
    const lp = listPrice(s.plan.kind);
    if (paid !== null) return { text: around((m) => t('settings.summary.plan_paid', { plan: name, price: m })), usd: paid };
    if (lp !== null) return { text: around((m) => t('settings.summary.plan_list', { plan: name, price: m })), usd: lp };
    return { text: [t('settings.summary.plan_open', { plan: name }), ''] as [string, string], usd: null };
  });

  const delivery = $derived(
    s.alerts.desktop_notifications && s.alerts.terminal_bell
      ? t('settings.summary.delivery_both')
      : s.alerts.desktop_notifications
        ? t('settings.summary.delivery_desktop')
        : s.alerts.terminal_bell
          ? t('settings.summary.delivery_bell')
          : t('settings.summary.delivery_none'),
  );

  const lines = $derived<{ id: string; icon: IconName; text?: string }[]>([
    {
      id: 'currency',
      icon: 'globe',
      text:
        s.currency.code === 'USD'
          ? t('settings.summary.currency_usd')
          : t('settings.summary.currency_other', { code: s.currency.code, rate: fmt.number(s.currency.usd_rate, 4) }),
    },
    { id: 'plan', icon: 'insights' },
    {
      id: 'alerts',
      icon: 'alerts',
      text: t('settings.summary.alerts', { thresholds: list(s.alerts.limit_thresholds.map((n) => fmt.pct(n))), delivery }),
    },
    {
      id: 'privacy',
      icon: 'lock',
      text: t('settings.summary.privacy', {
        titles: s.privacy.session_titles ? t('settings.summary.titles_on') : t('settings.summary.titles_off'),
        names: s.privacy.redact_paths ? t('settings.summary.names_on') : t('settings.summary.names_off'),
        viewer: s.privacy.conversation_viewer ? t('settings.summary.viewer_on') : t('settings.summary.viewer_off'),
      }),
    },
    {
      id: 'retention',
      icon: 'clock',
      text: s.retention.requests_days === 0 ? t('settings.summary.retention_forever') : t('settings.summary.retention_days', { days: fmt.number(s.retention.requests_days) }),
    },
  ]);
</script>

<section class="summary" aria-labelledby="summary-h">
  <h2 id="summary-h">{t('settings.summary.title')}</h2>
  <ul>
    {#each lines as l (l.id)}
      <li>
        <span class="glyph" aria-hidden="true"><Icon name={l.icon} size={17} /></span>
        <p>
          {#if l.id === 'plan'}
            {plan.text[0]}{#if plan.usd !== null}<Money usd={plan.usd} confidence="estimate" size="sm" badge="seal" />{/if}{plan.text[1]}
          {:else}{l.text}{/if}
        </p>
        <button type="button" class="change" onclick={() => goToSection(l.id)}>{t('settings.summary.change')}</button>
      </li>
    {/each}
  </ul>
</section>

<style>
  .summary {
    padding: var(--sp-5) var(--sp-6);
    border: 1px solid var(--line);
    border-left: 3px solid var(--accent);
    border-radius: var(--r-md);
    background: var(--surface);
  }
  h2 {
    font-size: var(--fs-md);
    margin-bottom: var(--sp-3);
  }
  li {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto;
    gap: var(--sp-3);
    align-items: start;
    padding: var(--sp-2) 0;
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--r-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }
  p {
    align-self: center;
    color: var(--ink);
    line-height: var(--lh-snug);
  }
  .change {
    align-self: center;
    padding: 4px var(--sp-2);
    border: 0;
    border-radius: var(--r-xs);
    background: none;
    color: var(--accent);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .change:hover {
    background: var(--accent-soft);
  }
  @media (max-width: 560px) {
    .summary {
      padding: var(--sp-4);
    }
  }
</style>
