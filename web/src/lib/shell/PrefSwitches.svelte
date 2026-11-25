<script lang="ts">
  import type { Theme } from '../api/contract';
  import { i18n, t, type Lang } from '../i18n/index.svelte';
  import { setLanguage, setTheme } from '../stores/app.svelte';
  import { theme } from '../stores/theme.svelte';
  import Icon, { type IconName } from '../ui/Icon.svelte';

  const THEMES: { id: Theme; icon: IconName; label: 'shell.theme_system' | 'shell.theme_light' | 'shell.theme_dark' }[] = [
    { id: 'system', icon: 'monitor', label: 'shell.theme_system' },
    { id: 'light', icon: 'sun', label: 'shell.theme_light' },
    { id: 'dark', icon: 'moon', label: 'shell.theme_dark' },
  ];
  const LANGS: Lang[] = ['en', 'pl'];
</script>

<div class="prefs">
  <div class="seg" role="radiogroup" aria-label={t('shell.language')}>
    {#each LANGS as l (l)}
      <button type="button" role="radio" aria-checked={i18n.lang === l} lang={l} title={t(`lang.${l}`)} onclick={() => setLanguage(l)}>{l.toUpperCase()}</button>
    {/each}
  </div>
  <div class="seg" role="radiogroup" aria-label={t('shell.theme')}>
    {#each THEMES as th (th.id)}
      <button type="button" role="radio" aria-checked={theme.pref === th.id} aria-label={t(th.label)} title={t(th.label)} onclick={() => setTheme(th.id)}>
        <Icon name={th.icon} size={16} />
      </button>
    {/each}
  </div>
</div>

<style>
  .prefs {
    display: inline-flex;
    gap: var(--sp-2);
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: var(--r-sm);
    background: var(--surface-3);
  }
  button {
    display: inline-grid;
    place-items: center;
    min-width: 30px;
    height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: calc(var(--r-sm) - 2px);
    background: none;
    color: var(--ink-2);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
    letter-spacing: 0.02em;
  }
  button:hover {
    color: var(--ink);
  }
  button[aria-checked='true'] {
    background: var(--surface);
    color: var(--ink);
    box-shadow: var(--shadow-raise);
  }
</style>
