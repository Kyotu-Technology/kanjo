<script lang="ts">
  import { t, tk } from '../lib/i18n/index.svelte';
  import { DAY_KEYS } from '../lib/i18n/format';
  import { app, updateSettings } from '../lib/stores/app.svelte';
  import PrefSwitches from '../lib/shell/PrefSwitches.svelte';
  import Card from '../lib/ui/Card.svelte';
  import InlineHelp from '../lib/ui/InlineHelp.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Select from '../lib/ui/Select.svelte';
  import Switch from '../lib/ui/Switch.svelte';

  const s = $derived(app.settings);
  let code = $state(app.settings?.currency.code ?? 'USD');
  let rate = $state(String(app.settings?.currency.usd_rate ?? 1));

  function saveCurrency() {
    const r = Number(rate.replace(',', '.'));
    if (!/^[A-Z]{3}$/.test(code) || !(r > 0)) return;
    void updateSettings((x) => ({ ...x, currency: { code, usd_rate: r } }));
  }
</script>

<PageHeader title={t('pages.settings.title')} lede={t('pages.settings.lede')} />

{#if s}
  <div class="stack">
    <Card title={t('pages.settings.appearance')}>
      <div class="row">
        <PrefSwitches />
        <Select
          label={t('pages.settings.week_start')}
          value={s.week_start}
          options={DAY_KEYS.map((d, i) => ({ value: i, label: tk(`chart.weekday_short.${d}`) }))}
          onchange={(v) => updateSettings((x) => ({ ...x, week_start: v }))}
        />
      </div>
    </Card>
    <Card title={t('pages.settings.currency')}>
      {#snippet actions()}<InlineHelp text={t('pages.settings.currency_help')} />{/snippet}
      <form
        class="row"
        onsubmit={(e) => {
          e.preventDefault();
          saveCurrency();
        }}
      >
        <label class="field"><span>{t('pages.settings.currency_code')}</span><input bind:value={code} maxlength="3" autocomplete="off" /></label>
        <label class="field"><span>{t('pages.settings.currency_rate')}</span><input bind:value={rate} inputmode="decimal" /></label>
        <button type="submit" class="save">{t('common.save')}</button>
      </form>
    </Card>
    <Card title={t('pages.settings.privacy')}>
      <Switch
        checked={s.privacy.session_titles}
        label={t('onboarding.privacy.titles')}
        description={t('onboarding.privacy.titles_help')}
        onchange={(v) => updateSettings((x) => ({ ...x, privacy: { ...x.privacy, session_titles: v } }))}
      />
      <Switch
        checked={s.privacy.redact_paths}
        label={t('onboarding.privacy.redact')}
        description={t('onboarding.privacy.redact_help')}
        onchange={(v) => updateSettings((x) => ({ ...x, privacy: { ...x.privacy, redact_paths: v } }))}
      />
      <Switch
        checked={s.privacy.conversation_viewer}
        label={t('onboarding.privacy.viewer')}
        description={t('onboarding.privacy.viewer_help')}
        onchange={(v) => updateSettings((x) => ({ ...x, privacy: { ...x.privacy, conversation_viewer: v } }))}
      />
    </Card>
    <p class="preview">{t('pages.preview')}</p>
  </div>
{/if}

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
    max-width: 820px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-5);
    align-items: flex-end;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    color: var(--ink-2);
  }
  .field input {
    min-height: 36px;
    width: 200px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--ink);
  }
  .save {
    min-height: 36px;
    padding: 0 var(--sp-4);
    border: 1px solid var(--accent);
    border-radius: var(--r-sm);
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: var(--fw-medium);
  }
  .preview {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
