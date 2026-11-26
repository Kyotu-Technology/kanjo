<script lang="ts">
  import type { Budget, Project } from '../../lib/api/contract';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import { api, cache, describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Money from '../../lib/ui/Money.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';
  import BudgetDialog from './BudgetDialog.svelte';

  interface Props {
    budgets: Budget[] | null;
    projects: Project[];
    refreshing?: boolean;
    highlight?: number | null;
  }
  let { budgets, projects, refreshing = false, highlight = null }: Props = $props();
  let editOpen = $state(false);
  let editing = $state<Budget | null>(null);
  let deleting = $state<Budget | null>(null);
  let deleteOpen = $state(false);
  let busy = $state(false);

  function create() {
    editing = null;
    editOpen = true;
  }
  function edit(b: Budget) {
    editing = b;
    editOpen = true;
  }
  function askDelete(b: Budget) {
    deleting = b;
    deleteOpen = true;
  }
  async function confirmDelete() {
    if (!deleting) return;
    busy = true;
    try {
      await api().deleteBudget(deleting.id);
      cache.invalidate(['budgets']);
      toasts.push('success', t('pages.alerts.budget.deleted'));
      deleteOpen = false;
    } catch (e) {
      toasts.push('error', describeError(e));
    } finally {
      busy = false;
    }
  }

  const scopeText = (b: Budget) =>
    b.scope === 'global' ? t('pages.alerts.budget.scope_global') : tk(`pages.alerts.budget.scope_${b.scope}`, { name: b.scope_label ?? b.scope_value ?? '' });
  const stateIcon = (s: Budget['state']) => (s === 'ok' ? 'check' : s === 'warning' ? 'warning' : 'critical');
  const geometry = (b: Budget) => {
    const projPct = b.amount_usd > 0 ? (b.projected_usd / b.amount_usd) * 100 : 0;
    const top = Math.max(100, b.spent_pct, projPct) * 1.04;
    return { top, spent: (Math.min(b.spent_pct, top) / top) * 100, amount: (100 / top) * 100, warn: (b.warn_pct / top) * 100, proj: (projPct / top) * 100 };
  };
</script>

<Card title={t('pages.alerts.budgets')} {refreshing}>
  {#snippet actions()}
    {#if budgets?.length}<Button size="sm" variant="primary" onclick={create}>{t('pages.alerts.budget.new')}</Button>{/if}
  {/snippet}
  {#if budgets === null}
    <Skeleton lines={5} />
  {:else if budgets.length}
    <ul class="budgets">
      {#each budgets as b (b.id)}
        {@const g = geometry(b)}
        {@const over = b.projected_usd - b.amount_usd}
        <li id="budget-{b.id}" class="budget {b.state}" class:paused={!b.enabled} class:hl={highlight === b.id} tabindex="-1">
          <div class="head">
            <h3>{b.name}</h3>
            {#if b.enabled}
              <span class="state"><Icon name={stateIcon(b.state)} size={14} />{t(`pages.alerts.state.${b.state}`)}</span>
            {:else}
              <span class="state off">{t('pages.alerts.budget.paused')}</span>
            {/if}
          </div>
          <p class="scope">
            <span>{scopeText(b)}</span>
            <span>{tk(`pages.alerts.budget.period_now.${b.period}`)}{#if b.period !== 'day'}, {t('pages.alerts.budget.ends', { date: fmt.date(b.period_end - 1, 'medium') })}{/if}</span>
          </p>
          <div class="amounts">
            <Money usd={b.spent_usd} confidence="lower_bound" size="lg" />
            <span class="of">{t('pages.alerts.spent_of', { spent: '', amount: fmt.money(b.amount_usd) }).trim()}</span>
          </div>
          <div
            class="track"
            role="meter"
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={Math.round(Math.min(100, b.spent_pct))}
            aria-valuetext={t('pages.alerts.budget.meter', { name: b.name, pct: fmt.pct(b.spent_pct) })}
            aria-label={b.name}
          >
            <span class="fill" style="width:{g.spent}%"></span>
            <span class="tick warn" style="left:{g.warn}%" title={t('pages.alerts.budget.warn_mark', { pct: fmt.pct(b.warn_pct) })}></span>
            <span class="tick amount" style="left:{g.amount}%"></span>
            {#if b.enabled}<span class="proj" style="left:{Math.min(g.proj, 100)}%" title={t('pages.alerts.budget.projected_mark')}></span>{/if}
          </div>
          <div class="legend">
            <span>{fmt.pct(b.spent_pct)}</span>
            <span class="key"><span class="k-warn" aria-hidden="true"></span>{t('pages.alerts.budget.warn_mark', { pct: fmt.pct(b.warn_pct) })}</span>
            {#if b.enabled}<span class="key"><span class="k-proj" aria-hidden="true"></span>{t('pages.alerts.budget.projected_mark')}</span>{/if}
          </div>
          <p class="pace" class:over={over > 0 && b.state !== 'exceeded'}>
            {#if over > 0 && b.state !== 'exceeded'}<Icon name="warning" size={15} />{/if}
            <span>
              {over > 0
                ? t('pages.alerts.budget.pace_over', { value: fmt.money(b.projected_usd), over: fmt.money(over) })
                : t('pages.alerts.budget.pace', { value: fmt.money(b.projected_usd) })}
              <ConfidenceBadge confidence="estimate" compact />
            </span>
          </p>
          <div class="foot">
            <span class="left">
              {b.spent_usd > b.amount_usd
                ? t('pages.alerts.budget.over_by', { value: fmt.money(b.spent_usd - b.amount_usd) })
                : t('pages.alerts.budget.left', { value: fmt.money(b.amount_usd - b.spent_usd) })}
            </span>
            <span class="btns">
              <Button size="sm" variant="ghost" onclick={() => edit(b)} aria-label={t('pages.alerts.budget.edit_named', { name: b.name })}>{t('pages.alerts.budget.edit')}</Button>
              <Button size="sm" variant="ghost" onclick={() => askDelete(b)} aria-label={t('pages.alerts.budget.delete_named', { name: b.name })}>{t('pages.alerts.budget.delete')}</Button>
            </span>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState icon="alerts" title={t('pages.alerts.no_budgets')} body={t('pages.alerts.budget.empty_body')}>
      {#snippet action()}<Button variant="primary" onclick={create}>{t('pages.alerts.budget.new')}</Button>{/snippet}
    </EmptyState>
  {/if}
</Card>

<BudgetDialog bind:open={editOpen} budget={editing} {projects} />

<Dialog bind:open={deleteOpen} title={t('pages.alerts.budget.delete_title')} description={deleting ? t('pages.alerts.budget.delete_body', { name: deleting.name }) : ''} size="sm">
  <span></span>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (deleteOpen = false)}>{t('common.cancel')}</Button>
    <Button variant="danger" {busy} onclick={confirmDelete}>{t('pages.alerts.budget.delete_confirm')}</Button>
  {/snippet}
</Dialog>

<style>
  .budgets {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: var(--sp-4);
  }
  .budget {
    --fill: var(--meter-fill);
    display: flex;
    flex-direction: column;
    padding: var(--sp-4) var(--sp-5);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
    scroll-margin-top: calc(var(--topbar-h) + var(--sp-4));
    transition: box-shadow var(--dur) var(--ease);
  }
  .budget:focus {
    outline: none;
  }
  .budget.hl {
    box-shadow: 0 0 0 2px var(--focus);
  }
  .warning {
    --fill: var(--warning);
  }
  .exceeded {
    --fill: var(--critical);
  }
  .paused {
    --fill: var(--ink-3);
  }
  .paused > :not(.foot) {
    opacity: 0.65;
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  h3 {
    font-size: var(--fs-md);
    font-weight: var(--fw-semibold);
    letter-spacing: 0;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    padding: 2px 8px;
    border-radius: var(--r-pill);
    background: var(--good-soft);
    color: var(--good-ink);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
  }
  .warning .state {
    background: var(--warning-soft);
    color: var(--warning-ink);
  }
  .exceeded .state {
    background: var(--critical-soft);
    color: var(--critical-ink);
  }
  .state.off {
    background: var(--surface-3);
    color: var(--ink-2);
  }
  .scope {
    display: flex;
    flex-direction: column;
    margin-top: 2px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .amounts {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--sp-1) var(--sp-2);
    margin-top: var(--sp-4);
  }
  .of {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .track {
    position: relative;
    height: 10px;
    margin-top: var(--sp-3);
    border-radius: var(--r-pill);
    background: var(--meter-track);
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    min-width: 10px;
    border-radius: inherit;
    background: var(--fill);
  }
  .tick {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
  }
  .tick.warn {
    background: var(--ink-3);
  }
  .tick.amount {
    background: var(--ink);
  }
  .proj {
    position: absolute;
    top: -6px;
    bottom: -6px;
    width: 0;
    margin-left: -1px;
    border-left: 2px dashed var(--ink);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1) var(--sp-4);
    margin-top: var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .legend > span:first-child {
    color: var(--ink);
    font-weight: var(--fw-semibold);
    margin-right: auto;
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .k-warn {
    width: 2px;
    height: 10px;
    background: var(--ink-3);
  }
  .k-proj {
    width: 0;
    height: 12px;
    border-left: 2px dashed var(--ink);
  }
  .pace {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin-top: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .pace.over {
    color: var(--serious-ink);
    font-weight: var(--fw-medium);
  }
  .pace :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-2);
    margin-top: auto;
    padding-top: var(--sp-3);
  }
  .left {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  .btns {
    display: flex;
    gap: var(--sp-1);
    margin-right: calc(-1 * var(--sp-2));
  }
</style>
