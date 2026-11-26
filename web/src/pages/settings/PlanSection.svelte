<script lang="ts">
  import type { PlanKind, Settings } from '../../lib/api/contract';
  import { t, tk } from '../../lib/i18n/index.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import { PLANS } from './plans';
  import SaveStatus from './SaveStatus.svelte';
  import { around, parseDecimal, Saver } from './saver.svelte';

  let { s }: { s: Settings } = $props();
  const saver = new Saver();
  const uid = $props.id();
  let kind = $state<PlanKind>('unknown');
  let price = $state('');
  let touched = $state(false);

  function reset() {
    kind = s.plan.kind;
    price = s.plan.monthly_usd === null ? '' : String(s.plan.monthly_usd);
    touched = false;
  }
  let synced = '';
  $effect.pre(() => {
    const key = JSON.stringify(s.plan);
    if (key !== synced) {
      synced = key;
      reset();
    }
  });

  const noPrice = $derived(kind === 'api' || kind === 'unknown');
  const paid = $derived(price.trim() === '' ? null : parseDecimal(price));
  const invalid = $derived(paid !== null && Number.isNaN(paid));
  const dirty = $derived(kind !== s.plan.kind || (noPrice ? null : paid) !== s.plan.monthly_usd);
  const list = $derived(PLANS.find((p) => p.kind === kind));

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    touched = true;
    if (invalid || !dirty) return;
    const monthly = noPrice ? null : paid;
    const ok = await saver.save((x) => ({ ...x, plan: { kind, monthly_usd: monthly } }), ['plan', 'usage']);
    if (!ok) reset();
  }
</script>

<Card id="plan" title={t('settings.plan.title')} subtitle={t('settings.plan.subtitle')}>
  {#snippet actions()}<SaveStatus state={saver.state} />{/snippet}
  <form onsubmit={submit} novalidate>
    <fieldset>
      <legend class="label-text">{t('settings.plan.kind')}</legend>
      <div class="plans">
        {#each PLANS as p (p.kind)}
          <label class="plan" class:selected={kind === p.kind}>
            <input type="radio" name="plan-{uid}" value={p.kind} checked={kind === p.kind} onchange={() => (kind = p.kind)} />
            <span class="name">{tk(`plan.${p.kind}`)}</span>
            <span class="price">
              {#if p.price !== null}
                {@const [a, b] = around((m) => t(p.seat ? 'settings.plan.list_seat' : 'settings.plan.list_price', { price: m }))}
                {a}<Money usd={p.price} confidence="estimate" size="sm" badge="seal" />{b}
              {:else}
                {t('settings.plan.no_list')}
              {/if}
            </span>
            {#if kind === p.kind}<span class="tick" aria-hidden="true"><Icon name="check" size={14} strokeWidth={2.6} /></span>{/if}
          </label>
        {/each}
      </div>
    </fieldset>

    {#if noPrice}
      {#if kind === 'api'}<p class="note api"><Icon name="info" size={16} /><span>{t('settings.plan.api_note')}</span></p>{/if}
    {:else}
      <div class="field paid">
        <label class="label" for="paid-{uid}">{t('settings.plan.paid')}</label>
        <span class="affix">
          <span aria-hidden="true">USD</span>
          <input
            id="paid-{uid}"
            class="input"
            inputmode="decimal"
            autocomplete="off"
            placeholder={list?.price != null ? String(list.price) : ''}
            bind:value={price}
            aria-invalid={touched && invalid}
            aria-describedby="paid-h-{uid}"
          />
        </span>
        {#if touched && invalid}<p class="err" role="alert">{t('settings.plan.paid_invalid')}</p>{/if}
        <p class="hint" id="paid-h-{uid}">{t('settings.plan.paid_help')}</p>
        {#if paid !== null && !invalid && s.currency.code !== 'USD'}
          {@const [a, b] = around((m) => t('settings.plan.paid_shown', { amount: m }))}
          <p class="hint shown">{a}<Money usd={paid} confidence="estimate" size="sm" badge="seal" />{b}</p>
        {/if}
      </div>
    {/if}

    <div class="form-foot">
      <Button type="submit" variant="primary" disabled={!dirty} busy={saver.state === 'saving'}>{t('settings.plan.save')}</Button>
      {#if dirty}<Button variant="ghost" onclick={reset}>{t('common.cancel')}</Button>{/if}
    </div>
  </form>
</Card>

<style>
  .plans {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: var(--sp-2);
    margin-top: var(--sp-2);
  }
  .plan {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--sp-3) var(--sp-4);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    background: var(--surface);
    cursor: pointer;
  }
  .plan:hover {
    border-color: var(--control-border);
  }
  .plan.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .plan:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .plan input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .name {
    font-weight: var(--fw-semibold);
  }
  .price {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .tick {
    position: absolute;
    top: var(--sp-3);
    right: var(--sp-3);
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .paid {
    margin-top: var(--sp-5);
  }
  .api {
    margin-top: var(--sp-4);
  }
  .shown {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 4px;
  }
</style>
