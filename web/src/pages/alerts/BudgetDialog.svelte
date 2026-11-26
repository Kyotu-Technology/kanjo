<script lang="ts">
  import { untrack } from 'svelte';
  import type { Budget, BudgetInput, BudgetPeriod, BudgetScope, Project } from '../../lib/api/contract';
  import { i18n, t } from '../../lib/i18n/index.svelte';
  import { api, cache, describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';
  import Select from '../../lib/ui/Select.svelte';
  import Switch from '../../lib/ui/Switch.svelte';

  interface Props {
    open: boolean;
    budget: Budget | null;
    projects: Project[];
  }
  let { open = $bindable(), budget, projects }: Props = $props();
  const uid = $props.id();

  let name = $state('');
  let scope = $state<BudgetScope>('global');
  let project = $state('');
  let client = $state('');
  let account = $state('');
  let period = $state<BudgetPeriod>('month');
  let amount = $state('');
  let warn = $state('80');
  let enabled = $state(true);
  let tried = $state(false);
  let busy = $state(false);

  const rate = $derived(i18n.currency.usd_rate > 0 ? i18n.currency.usd_rate : 1);
  const clients = $derived([...new Set(projects.map((p) => p.client).filter((c): c is string => !!c))].sort());

  function reset(b: Budget | null) {
    name = b?.name ?? '';
    scope = b?.scope ?? 'global';
    project = b?.scope === 'project' ? (b.scope_value ?? '') : String(projects[0]?.id ?? '');
    client = b?.scope === 'client' ? (b.scope_value ?? '') : (clients[0] ?? '');
    account = b?.scope === 'account' ? (b.scope_value ?? '') : '';
    period = b?.period ?? 'month';
    amount = b ? String(Math.round(b.amount_usd * rate * 100) / 100) : '';
    warn = String(b?.warn_pct ?? 80);
    enabled = b?.enabled ?? true;
    tried = false;
  }

  $effect(() => {
    if (!open) return;
    const b = budget;
    untrack(() => reset(b));
  });

  const scopeValue = $derived(scope === 'project' ? project : scope === 'client' ? client : scope === 'account' ? account.trim() : null);
  const amountNum = $derived(Number(amount.replace(',', '.')));
  const warnNum = $derived(Number(warn));
  const errors = $derived({
    name: name.trim() ? '' : t('pages.alerts.budget.invalid_name'),
    scope: scope !== 'global' && !scopeValue ? t('pages.alerts.budget.invalid_scope') : '',
    amount: Number.isFinite(amountNum) && amountNum > 0 ? '' : t('pages.alerts.budget.invalid_amount'),
    warn: Number.isInteger(warnNum) && warnNum >= 1 && warnNum <= 100 ? '' : t('pages.alerts.budget.invalid_warn'),
  });
  const valid = $derived(!errors.name && !errors.scope && !errors.amount && !errors.warn);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    tried = true;
    if (!valid) return;
    const input: BudgetInput = {
      name: name.trim(),
      scope,
      scope_value: scopeValue,
      period,
      amount_usd: amountNum / rate,
      warn_pct: warnNum,
      enabled,
    };
    busy = true;
    try {
      if (budget) await api().updateBudget(budget.id, input);
      else await api().createBudget(input);
      cache.invalidate(['budgets']);
      toasts.push('success', budget ? t('pages.alerts.budget.saved') : t('pages.alerts.budget.created'));
      open = false;
    } catch (err) {
      toasts.push('error', describeError(err));
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={budget ? t('pages.alerts.budget.edit_title') : t('pages.alerts.budget.new_title')} description={t('pages.alerts.budget.dialog_body')} size="md">
  <form id="budget-{uid}" class="form" novalidate onsubmit={save}>
    <div class="field">
      <label for="bn-{uid}">{t('pages.alerts.budget.name')}</label>
      <input id="bn-{uid}" type="text" bind:value={name} placeholder={t('pages.alerts.budget.name_ph')} aria-invalid={tried && !!errors.name} aria-describedby={tried && errors.name ? `bne-${uid}` : undefined} />
      {#if tried && errors.name}<p id="bne-{uid}" class="err">{errors.name}</p>{/if}
    </div>
    <div class="row">
      <Select
        label={t('pages.alerts.budget.scope')}
        bind:value={scope}
        options={(['global', 'project', 'client', 'account'] as const).map((s) => ({ value: s, label: t(`pages.alerts.budget.scope_opt.${s}`) }))}
      />
      {#if scope === 'project'}
        <Select label={t('pages.alerts.budget.project')} bind:value={project} options={projects.map((p) => ({ value: String(p.id), label: p.name }))} />
      {:else if scope === 'client'}
        {#if clients.length}
          <Select label={t('pages.alerts.budget.client')} bind:value={client} options={clients.map((c) => ({ value: c, label: c }))} />
        {:else}
          <p class="hint">{t('pages.alerts.budget.no_clients')}</p>
        {/if}
      {:else if scope === 'account'}
        <div class="field">
          <label for="ba-{uid}">{t('pages.alerts.budget.account')}</label>
          <input id="ba-{uid}" type="text" bind:value={account} autocomplete="off" spellcheck="false" aria-describedby="bah-{uid}" aria-invalid={tried && !!errors.scope} />
        </div>
      {/if}
    </div>
    {#if scope === 'account'}<p id="bah-{uid}" class="hint">{t('pages.alerts.budget.account_help')}</p>{/if}
    {#if tried && errors.scope}<p class="err">{errors.scope}</p>{/if}
    <div class="row three">
      <Select
        label={t('pages.alerts.budget.period')}
        bind:value={period}
        options={(['day', 'week', 'month'] as const).map((p) => ({ value: p, label: t(`pages.alerts.budget.period_opt.${p}`) }))}
      />
      <div class="field">
        <label for="bm-{uid}">{t('pages.alerts.budget.amount', { currency: i18n.currency.code || 'USD' })}</label>
        <input id="bm-{uid}" type="text" inputmode="decimal" bind:value={amount} aria-invalid={tried && !!errors.amount} />
      </div>
      <div class="field">
        <label for="bw-{uid}">{t('pages.alerts.budget.warn')}</label>
        <span class="suffix">
          <input id="bw-{uid}" type="number" min="1" max="100" step="1" bind:value={warn} aria-invalid={tried && !!errors.warn} aria-describedby="bwh-{uid}" />
          <span aria-hidden="true">%</span>
        </span>
        <span id="bwh-{uid}" class="sr-only">{t('pages.alerts.budget.warn_help')}</span>
      </div>
    </div>
    {#if tried && (errors.amount || errors.warn)}<p class="err">{errors.amount || errors.warn}</p>{/if}
    <Switch bind:checked={enabled} label={t('pages.alerts.budget.enabled')} description={t('pages.alerts.budget.enabled_help')} />
  </form>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" type="submit" form="budget-{uid}" {busy}>{budget ? t('pages.alerts.budget.save') : t('pages.alerts.budget.create')}</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  .row {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-4);
    align-items: end;
  }
  .row.three {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr) minmax(0, 0.8fr);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  label {
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    color: var(--ink-2);
  }
  input {
    min-height: 36px;
    width: 100%;
    padding: 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  input::placeholder {
    color: var(--ink-3);
  }
  input[aria-invalid='true'] {
    border-color: var(--critical);
  }
  .suffix {
    position: relative;
    display: flex;
  }
  .suffix input {
    padding-right: 28px;
  }
  .suffix span {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--ink-3);
    font-size: var(--fs-sm);
    pointer-events: none;
  }
  .row :global(.field) {
    width: 100%;
  }
  .hint {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .err {
    color: var(--critical-ink);
    font-size: var(--fs-sm);
  }
  @media (max-width: 560px) {
    .row,
    .row.three {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
