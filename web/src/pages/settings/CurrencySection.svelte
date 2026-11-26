<script lang="ts">
  import type { Settings } from '../../lib/api/contract';
  import { formatMoney } from '../../lib/i18n/format';
  import { i18n, t } from '../../lib/i18n/index.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import SaveStatus from './SaveStatus.svelte';
  import { parseDecimal, Saver } from './saver.svelte';

  let { s }: { s: Settings } = $props();
  const saver = new Saver();
  const uid = $props.id();
  const COMMON = ['USD', 'EUR', 'GBP', 'PLN', 'CHF', 'CZK', 'SEK', 'NOK', 'DKK', 'CAD', 'AUD', 'JPY'];
  let code = $state('USD');
  let rate = $state('1');
  let touched = $state(false);

  function reset() {
    code = s.currency.code;
    rate = String(s.currency.usd_rate).replace('.', i18n.lang === 'pl' ? ',' : '.');
    touched = false;
  }
  let synced = '';
  $effect.pre(() => {
    const key = JSON.stringify(s.currency);
    if (key !== synced) {
      synced = key;
      reset();
    }
  });

  const norm = $derived(code.trim().toUpperCase());
  const isUsd = $derived(norm === 'USD');
  const codeOk = $derived(/^[A-Z]{3}$/.test(norm));
  const r = $derived(isUsd ? 1 : parseDecimal(rate));
  const rateOk = $derived(r > 0);
  const dirty = $derived(norm !== s.currency.code || (rateOk && r !== s.currency.usd_rate));
  const example = $derived(codeOk && rateOk ? formatMoney(100, { code: norm, usd_rate: r }, i18n.locale, { digits: 2 }) : null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    touched = true;
    if (!codeOk || !rateOk || !dirty) return;
    const next = { code: norm, usd_rate: r };
    const ok = await saver.save((x) => ({ ...x, currency: next }));
    if (!ok) reset();
  }
</script>

<Card id="currency" title={t('settings.currency.title')} subtitle={t('settings.currency.subtitle')}>
  {#snippet actions()}<SaveStatus state={saver.state} />{/snippet}
  <form onsubmit={submit} novalidate>
    <div class="grid">
      <div class="field">
        <label class="label" for="code-{uid}">{t('settings.currency.code')}</label>
        <input
          id="code-{uid}"
          class="input short"
          list="codes-{uid}"
          maxlength="3"
          autocomplete="off"
          autocapitalize="characters"
          spellcheck="false"
          bind:value={code}
          aria-invalid={touched && !codeOk}
          aria-describedby="code-h-{uid}"
        />
        <datalist id="codes-{uid}">
          {#each COMMON as c (c)}<option value={c}></option>{/each}
        </datalist>
        <p class="hint" id="code-h-{uid}">{t('settings.currency.code_help')}</p>
        {#if touched && !codeOk}<p class="err" role="alert">{t('settings.currency.code_invalid')}</p>{/if}
      </div>
      <div class="field">
        <label class="label" for="rate-{uid}">{t('settings.currency.rate')}</label>
        <span class="affix">
          <span aria-hidden="true">{t('settings.currency.rate_eq')}</span>
          <input
            id="rate-{uid}"
            class="input rate"
            inputmode="decimal"
            autocomplete="off"
            disabled={isUsd}
            value={isUsd ? '1' : rate}
            oninput={(e) => (rate = e.currentTarget.value)}
            aria-invalid={touched && !rateOk}
          />
          <span aria-hidden="true">{codeOk ? norm : ''}</span>
        </span>
        {#if isUsd}<p class="hint">{t('settings.currency.usd_note')}</p>{/if}
        {#if touched && !rateOk}<p class="err" role="alert">{t('settings.currency.rate_invalid')}</p>{/if}
      </div>
    </div>
    {#if example && !isUsd}
      <p class="example">{t('settings.currency.example', { usd: formatMoney(100, { code: 'USD', usd_rate: 1 }, i18n.locale, { digits: 2 }), local: example })}</p>
    {/if}
    <p class="note never"><Icon name="lock" size={16} /><span>{t('settings.currency.never')}</span></p>
    <div class="form-foot">
      <Button type="submit" variant="primary" disabled={!dirty} busy={saver.state === 'saving'}>{t('settings.currency.save')}</Button>
      {#if dirty}<Button variant="ghost" onclick={reset}>{t('common.cancel')}</Button>{/if}
    </div>
  </form>
</Card>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--sp-5);
  }
  .input.rate {
    max-width: 130px;
  }
  .example {
    margin-top: var(--sp-4);
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }
  .never {
    margin-top: var(--sp-4);
  }
</style>
