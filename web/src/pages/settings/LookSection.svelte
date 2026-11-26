<script lang="ts">
  import type { Language, Settings, Theme } from '../../lib/api/contract';
  import { t, tk } from '../../lib/i18n/index.svelte';
  import { DAY_KEYS } from '../../lib/i18n/format';
  import Card from '../../lib/ui/Card.svelte';
  import Icon, { type IconName } from '../../lib/ui/Icon.svelte';
  import Select from '../../lib/ui/Select.svelte';
  import SaveStatus from './SaveStatus.svelte';
  import { Saver } from './saver.svelte';

  let { s }: { s: Settings } = $props();
  const saver = new Saver();
  const LANGS: Language[] = ['auto', 'en', 'pl'];
  const THEMES: { id: Theme; icon: IconName }[] = [
    { id: 'system', icon: 'monitor' },
    { id: 'light', icon: 'sun' },
    { id: 'dark', icon: 'moon' },
  ];
</script>

<Card id="look" title={t('settings.look.title')}>
  {#snippet actions()}<SaveStatus state={saver.state} />{/snippet}
  <div class="grid">
    <Select
      label={t('settings.look.language')}
      value={s.language}
      options={LANGS.map((l) => ({ value: l, label: t(`lang.${l}`) }))}
      onchange={(v) => saver.save((x) => ({ ...x, language: v }))}
    />
    <fieldset class="field theme">
      <legend class="label-text">{t('settings.look.theme')}</legend>
      <div class="seg">
        {#each THEMES as th (th.id)}
          <label>
            <input type="radio" name="theme" value={th.id} checked={s.theme === th.id} onchange={() => saver.save((x) => ({ ...x, theme: th.id }))} />
            <Icon name={th.icon} size={16} />{t(`shell.theme_${th.id}`)}
          </label>
        {/each}
      </div>
    </fieldset>
    <div class="field">
      <Select
        label={t('settings.look.week_start')}
        value={s.week_start}
        options={DAY_KEYS.map((d, i) => ({ value: i, label: tk(`settings.weekday.${d}`) }))}
        onchange={(v) => saver.save((x) => ({ ...x, week_start: v }), ['usage', 'budgets'])}
      />
      <p class="hint">{t('settings.look.week_help')}</p>
    </div>
  </div>
  <p class="keys">
    {t('shortcuts.hint', { key: '?' })}
    <button type="button" onclick={() => window.dispatchEvent(new CustomEvent('kanjo:shortcuts'))}>{t('shortcuts.open')}</button>
  </p>
</Card>

<style>
  .keys {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
    margin-top: var(--sp-5);
    padding-top: var(--sp-4);
    border-top: 1px solid var(--line);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .keys button {
    padding: 2px var(--sp-2);
    border: 0;
    border-radius: var(--r-xs);
    background: none;
    color: var(--accent);
    font-weight: var(--fw-medium);
  }
  .keys button:hover {
    background: var(--accent-soft);
  }
  .theme {
    grid-column: 1 / -1;
    order: 3;
    align-items: flex-start;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--sp-5);
    align-items: start;
  }
</style>
