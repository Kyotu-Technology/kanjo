<script lang="ts">
  import type { RetentionSettings, Settings } from '../../lib/api/contract';
  import { t, tn } from '../../lib/i18n/index.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import SaveStatus from './SaveStatus.svelte';
  import { Saver } from './saver.svelte';

  let { s }: { s: Settings } = $props();
  const saver = new Saver();
  const uid = $props.id();
  type Key = keyof RetentionSettings;
  const ROWS: { key: Key; name: 'requests' | 'aggregates' | 'limit_samples' | 'alerts' | 'audit' | 'quarantine' }[] = [
    { key: 'requests_days', name: 'requests' },
    { key: 'aggregates_days', name: 'aggregates' },
    { key: 'limit_samples_days', name: 'limit_samples' },
    { key: 'alerts_days', name: 'alerts' },
    { key: 'audit_days', name: 'audit' },
    { key: 'quarantine_days', name: 'quarantine' },
  ];
  type Draft = Record<Key, { days: string; forever: boolean }>;
  let draft = $state<Draft>({} as Draft);
  let touched = $state(false);

  function reset() {
    const d = {} as Draft;
    for (const r of ROWS) {
      const v = s.retention[r.key];
      d[r.key] = { days: v === 0 ? '' : String(v), forever: v === 0 };
    }
    draft = d;
    touched = false;
  }
  let synced = '';
  $effect.pre(() => {
    const key = JSON.stringify(s.retention);
    if (key !== synced) {
      synced = key;
      reset();
    }
  });

  const value = (k: Key): number => {
    const d = draft[k];
    if (!d) return s.retention[k];
    if (d.forever) return 0;
    const n = Number(d.days.trim());
    return Number.isInteger(n) && n >= 1 && n <= 36_500 ? n : NaN;
  };
  const values = $derived(Object.fromEntries(ROWS.map((r) => [r.key, value(r.key)])) as Record<Key, number>);
  const bad = $derived(ROWS.filter((r) => Number.isNaN(values[r.key])).map((r) => r.key));
  const dirty = $derived(ROWS.some((r) => values[r.key] !== s.retention[r.key]));
  const life = (v: number) => (v === 0 ? Infinity : v);
  const shorter = $derived(ROWS.some((r) => !Number.isNaN(values[r.key]) && life(values[r.key]) < life(s.retention[r.key])));

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    touched = true;
    if (bad.length || !dirty) return;
    const next = { ...values };
    const ok = await saver.save((x) => ({ ...x, retention: next }));
    if (!ok) reset();
  }
</script>

<Card id="retention" title={t('settings.retention.title')} subtitle={t('settings.retention.subtitle')}>
  {#snippet actions()}<SaveStatus state={saver.state} />{/snippet}
  <form onsubmit={submit} novalidate>
    <ul class="list">
      {#each ROWS as r (r.key)}
        {@const d = draft[r.key]}
        {#if d}
          <li class="item">
            <div class="text">
              <label class="label-text" for="{r.key}-{uid}">{t(`settings.retention.${r.name}`)}</label>
              <p class="hint">{t(`settings.retention.${r.name}_help`)}</p>
              <p class="kept">
                {#if Number.isNaN(values[r.key])}&nbsp;{:else if values[r.key] === 0}{t('settings.retention.kept_forever')}{:else}{tn('settings.retention.kept_days', values[r.key])}{/if}
              </p>
            </div>
            <div class="ctl">
              <span class="affix">
                <input
                  id="{r.key}-{uid}"
                  class="input short"
                  inputmode="numeric"
                  maxlength="5"
                  disabled={d.forever}
                  bind:value={d.days}
                  aria-invalid={touched && bad.includes(r.key)}
                  placeholder={d.forever ? '' : '365'}
                />
                <span aria-hidden="true">{t('settings.retention.days')}</span>
              </span>
              <label class="forever">
                <input type="checkbox" bind:checked={d.forever} />
                {t('settings.retention.forever')}
              </label>
            </div>
            {#if touched && bad.includes(r.key)}<p class="err" role="alert">{t('settings.retention.invalid')}</p>{/if}
          </li>
        {/if}
      {/each}
    </ul>
    {#if shorter && dirty}<p class="note warn"><Icon name="warning" size={16} /><span>{t('settings.retention.shorter')}</span></p>{/if}
    <div class="form-foot">
      <Button type="submit" variant="primary" disabled={!dirty} busy={saver.state === 'saving'}>{t('settings.retention.save')}</Button>
      {#if dirty}<Button variant="ghost" onclick={reset}>{t('common.cancel')}</Button>{/if}
    </div>
  </form>
</Card>

<style>
  .list {
    border-top: 1px solid var(--line);
  }
  .item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--sp-2) var(--sp-5);
    align-items: start;
    padding: var(--sp-4) 0;
    border-bottom: 1px solid var(--line);
  }
  .item .err {
    grid-column: 1 / -1;
  }
  .kept {
    margin-top: 2px;
    color: var(--ink-2);
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
  }
  .ctl {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--sp-2);
  }
  .forever {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .forever input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }
  .note {
    margin-top: var(--sp-4);
  }
  @media (max-width: 560px) {
    .item {
      grid-template-columns: minmax(0, 1fr);
    }
    .ctl {
      flex-direction: row;
      align-items: center;
      gap: var(--sp-4);
    }
  }
</style>
