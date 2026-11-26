<script lang="ts">
  import type { AlertSettings, Settings } from '../../lib/api/contract';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Switch from '../../lib/ui/Switch.svelte';
  import SaveStatus from './SaveStatus.svelte';
  import { Saver } from './saver.svelte';

  let { s }: { s: Settings } = $props();
  const saver = new Saver();
  const uid = $props.id();
  const a = $derived(s.alerts);
  let draft = $state('');
  let addError = $state<string | null>(null);
  let ctx = $state('');
  let ctxError = $state(false);
  let synced = -1;
  $effect.pre(() => {
    if (s.alerts.context_threshold !== synced) {
      synced = s.alerts.context_threshold;
      ctx = String(synced);
      ctxError = false;
    }
  });

  type Flag = 'forecast' | 'budgets' | 'anomalies' | 'cache' | 'context' | 'desktop_notifications' | 'terminal_bell';
  const KINDS: { key: Flag; label: 'forecast' | 'budgets' | 'anomalies' | 'cache' }[] = [
    { key: 'forecast', label: 'forecast' },
    { key: 'budgets', label: 'budgets' },
    { key: 'anomalies', label: 'anomalies' },
    { key: 'cache', label: 'cache' },
  ];

  function patch(p: Partial<AlertSettings>) {
    return saver.save((x) => ({ ...x, alerts: { ...x.alerts, ...p } }), ['alerts']);
  }

  function addThreshold(e: SubmitEvent) {
    e.preventDefault();
    const n = Number(draft.trim());
    if (!Number.isInteger(n) || n < 1 || n > 100 || a.limit_thresholds.includes(n)) {
      addError = t('settings.alerts.threshold_invalid');
      return;
    }
    addError = null;
    draft = '';
    void patch({ limit_thresholds: [...a.limit_thresholds, n].sort((x, y) => x - y) });
  }

  function remove(n: number) {
    if (a.limit_thresholds.length <= 1) {
      addError = t('settings.alerts.threshold_last');
      return;
    }
    addError = null;
    void patch({ limit_thresholds: a.limit_thresholds.filter((x) => x !== n) });
  }

  function saveCtx() {
    const n = Number(ctx.trim());
    if (!Number.isInteger(n) || n < 10 || n > 100) {
      ctxError = true;
      return;
    }
    ctxError = false;
    if (n !== a.context_threshold) void patch({ context_threshold: n });
  }
</script>

<Card id="alerts" title={t('settings.alerts.title')} subtitle={t('settings.alerts.subtitle')}>
  {#snippet actions()}<SaveStatus state={saver.state} />{/snippet}
  <div class="block">
    <h3 class="subhead" id="th-{uid}">{t('settings.alerts.thresholds')}</h3>
    <p class="hint">{t('settings.alerts.thresholds_help')}</p>
    <ul class="chips" aria-labelledby="th-{uid}">
      {#each a.limit_thresholds as n (n)}
        <li class="chip">
          <span class="num">{fmt.pct(n)}</span>
          <button type="button" aria-label={t('settings.alerts.remove', { pct: fmt.pct(n) })} onclick={() => remove(n)}><Icon name="x" size={14} strokeWidth={2.2} /></button>
        </li>
      {/each}
      <li>
        <form class="add" onsubmit={addThreshold} novalidate>
          <label class="sr-only" for="add-{uid}">{t('settings.alerts.add_label')}</label>
          <input id="add-{uid}" class="input" inputmode="numeric" placeholder="75" maxlength="3" bind:value={draft} aria-invalid={!!addError} />
          <span class="pct" aria-hidden="true">%</span>
          <Button type="submit" size="sm">{t('settings.alerts.add')}</Button>
        </form>
      </li>
    </ul>
    {#if addError}<p class="err" role="alert">{addError}</p>{/if}
  </div>

  <div class="rows">
    {#each KINDS as k (k.key)}
      <Switch checked={a[k.key]} label={t(`settings.alerts.${k.label}`)} description={t(`settings.alerts.${k.label}_help`)} onchange={(v) => patch({ [k.key]: v })} />
    {/each}
    <div class="ctx">
      <Switch checked={a.context} label={t('settings.alerts.context')} description={t('settings.alerts.context_help')} onchange={(v) => patch({ context: v })} />
      <div class="ctx-field">
        <label class="label-text" for="ctx-{uid}">{t('settings.alerts.context_threshold')}</label>
        <span class="affix">
          <input
            id="ctx-{uid}"
            class="input short"
            inputmode="numeric"
            maxlength="3"
            disabled={!a.context}
            bind:value={ctx}
            onblur={saveCtx}
            onkeydown={(e) => e.key === 'Enter' && saveCtx()}
            aria-invalid={ctxError}
          />%
        </span>
        {#if ctxError}<p class="err" role="alert">{t('settings.alerts.context_invalid')}</p>{/if}
      </div>
    </div>
  </div>

  <h3 class="subhead">{t('settings.alerts.delivery')}</h3>
  <div class="rows">
    <Switch checked={a.desktop_notifications} label={t('settings.alerts.desktop')} description={t('settings.alerts.desktop_help')} onchange={(v) => patch({ desktop_notifications: v })} />
    <Switch checked={a.terminal_bell} label={t('settings.alerts.bell')} description={t('settings.alerts.bell_help')} onchange={(v) => patch({ terminal_bell: v })} />
  </div>
  <p class="note in-app"><Icon name="alerts" size={16} /><span>{t('settings.alerts.in_app')}</span></p>
</Card>

<style>
  .block {
    margin-bottom: var(--sp-5);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
    margin-top: var(--sp-3);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 34px;
    padding: 0 4px 0 var(--sp-3);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-pill);
    background: var(--surface-2);
    font-weight: var(--fw-semibold);
  }
  .chip button {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--ink-3);
  }
  .chip button:hover {
    background: var(--surface-3);
    color: var(--critical-ink);
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .add .input {
    width: 64px;
    min-height: 34px;
  }
  .pct {
    margin-left: -4px;
    color: var(--ink-3);
  }
  .rows {
    border-top: 1px solid var(--line);
  }
  .rows > :global(*) {
    padding: var(--sp-3) 0;
  }
  .ctx {
    display: flex;
    flex-direction: column;
  }
  .ctx > :global(*) {
    padding: 0;
  }
  .ctx-field {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2) var(--sp-3);
    margin-top: var(--sp-2);
  }
  .ctx-field .err {
    flex-basis: 100%;
  }
  .in-app {
    margin-top: var(--sp-4);
  }
</style>
