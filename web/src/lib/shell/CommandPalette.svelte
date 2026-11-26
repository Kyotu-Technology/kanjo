<script lang="ts">
  import type { SearchResult } from '../api/contract';
  import { i18n, t, type Lang } from '../i18n/index.svelte';
  import { href } from '../router';
  import { api, app, setLanguage, setTheme } from '../stores/app.svelte';
  import { router, navigate } from '../stores/router.svelte';
  import { theme } from '../stores/theme.svelte';
  import Icon, { type IconName } from '../ui/Icon.svelte';
  import { runExport } from './actions';
  import { pageExport } from './exports.svelte';
  import { NAV_ALL } from './nav';

  interface Item {
    id: string;
    group: 'pages' | 'actions' | 'results';
    label: string;
    sublabel?: string | null;
    icon: IconName;
    run: () => void;
  }

  let query = $state('');
  let results = $state<SearchResult[]>([]);
  let searching = $state(false);
  let activeIndex = $state(0);
  let input: HTMLInputElement | undefined = $state();
  let dialog: HTMLDialogElement | undefined = $state();
  let seq = 0;

  const norm = (s: string) => s.toLocaleLowerCase().normalize('NFD').replace(/\p{Diacritic}/gu, '');
  const matches = (s: string) => !query.trim() || norm(s).includes(norm(query.trim()));

  const otherLang: Lang = $derived(i18n.lang === 'en' ? 'pl' : 'en');
  const nextTheme = $derived(theme.resolved === 'dark' ? 'light' : 'dark');

  const items = $derived.by<Item[]>(() => {
    const pages: Item[] = NAV_ALL.filter((n) => matches(t(n.label))).map((n) => ({
      id: `page:${n.route}`,
      group: 'pages',
      label: t(n.label),
      icon: n.icon,
      run: () => navigate(href(n.path)),
    }));
    const exp = pageExport.current;
    const actions: Item[] = [
      {
        id: 'act:theme',
        group: 'actions' as const,
        label: t('palette.toggle_theme', { theme: t(nextTheme === 'dark' ? 'shell.theme_dark' : 'shell.theme_light').toLocaleLowerCase(i18n.locale) }),
        icon: (nextTheme === 'dark' ? 'moon' : 'sun') as IconName,
        run: () => void setTheme(nextTheme),
      },
      {
        id: 'act:lang',
        group: 'actions' as const,
        label: t('palette.switch_language', { language: t(`lang.${otherLang}`) }),
        icon: 'globe' as IconName,
        run: () => void setLanguage(otherLang),
      },
      ...(exp ? [{ id: 'act:export', group: 'actions' as const, label: t('palette.export_view'), icon: 'download' as IconName, run: () => void runExport(exp) }] : []),
    ].filter((a) => matches(a.label));
    const found: Item[] = results.map((r) => ({
      id: `res:${r.kind}:${r.id}`,
      group: 'results',
      label: r.label,
      sublabel: r.sublabel ?? t(`palette.kind.${r.kind as 'session'}`),
      icon: r.kind === 'session' ? 'sessions' : r.kind === 'project' ? 'projects' : r.kind === 'model' ? 'usage' : 'chevron_right',
      run: () => {
        if (r.kind === 'session') navigate(href(`/sessions/${encodeURIComponent(r.id)}`));
        else if (r.kind === 'project') navigate(href('/projects', { id: r.id }));
        else if (r.kind === 'model') navigate(href('/usage', { model: r.id }));
        else navigate(href(r.id.startsWith('/') ? r.id : `/${r.id}`));
      },
    }));
    return [...pages, ...actions, ...found];
  });

  const groups = $derived(
    (['pages', 'actions', 'results'] as const)
      .map((g) => ({ g, list: items.filter((i) => i.group === g) }))
      .filter((x) => x.list.length > 0),
  );

  $effect(() => {
    const q = query.trim();
    activeIndex = 0;
    if (q.length < 2) {
      results = [];
      searching = false;
      return;
    }
    const my = ++seq;
    searching = true;
    const timer = setTimeout(async () => {
      try {
        const r = await api().search(q);
        if (my === seq) results = r;
      } catch {
        if (my === seq) results = [];
      } finally {
        if (my === seq) searching = false;
      }
    }, 160);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    if (!dialog) return;
    if (app.paletteOpen && !dialog.open) {
      query = '';
      results = [];
      dialog.showModal();
      queueMicrotask(() => input?.focus());
    } else if (!app.paletteOpen && dialog.open) dialog.close();
  });

  function run(item: Item | undefined) {
    if (!item) return;
    app.paletteOpen = false;
    item.run();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      activeIndex = Math.min(items.length - 1, activeIndex + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      activeIndex = Math.max(0, activeIndex - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      run(items[activeIndex]);
    }
  }

  $effect(() => {
    void activeIndex;
    dialog?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  });
</script>

<dialog
  bind:this={dialog}
  class="palette"
  aria-label={t('palette.label')}
  onclose={() => (app.paletteOpen = false)}
  onclick={(e) => {
    if (e.target === dialog) app.paletteOpen = false;
  }}
>
  <div class="box">
    <div class="field">
      <Icon name="search" size={18} />
      <input
        bind:this={input}
        bind:value={query}
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={items[activeIndex] ? `pi-${activeIndex}` : undefined}
        aria-autocomplete="list"
        aria-label={t('palette.label')}
        placeholder={t('palette.placeholder')}
        autocomplete="off"
        spellcheck="false"
        onkeydown={onKey}
      />
      {#if searching}<span class="searching">{t('palette.searching')}</span>{/if}
    </div>
    <div id="palette-list" class="list" role="listbox" aria-label={t('palette.label')}>
      {#each groups as grp (grp.g)}
        <div role="group" aria-labelledby="pg-{grp.g}">
          <p class="group" id="pg-{grp.g}">{t(`palette.${grp.g}`)}</p>
          {#each grp.list as item (item.id)}
            {@const idx = items.indexOf(item)}
            <div
              id="pi-{idx}"
              role="option"
              tabindex="-1"
              aria-selected={idx === activeIndex}
              class="item"
              onpointermove={() => (activeIndex = idx)}
              onclick={() => run(item)}
              onkeydown={(e) => e.key === 'Enter' && run(item)}
            >
              <Icon name={item.icon} size={17} />
              <span class="label">{item.label}</span>
              {#if item.sublabel}<span class="sub">{item.sublabel}</span>{/if}
            </div>
          {/each}
        </div>
      {/each}
      {#if !items.length && !searching}
        <p class="empty">{t('palette.empty', { query })}</p>
      {/if}
    </div>
    <div class="hints" aria-hidden="true">
      <span><kbd>Up</kbd><kbd>Down</kbd> {t('palette.hint_move')}</span>
      <span><kbd>Enter</kbd> {t('palette.hint_open')}</span>
      <span><kbd>Esc</kbd> {t('palette.hint_close')}</span>
    </div>
  </div>
</dialog>

<style>
  .palette {
    margin-top: min(14vh, 140px);
    padding: 0;
    width: min(620px, calc(100vw - 24px));
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--surface);
    color: var(--ink);
    box-shadow: var(--shadow-overlay);
  }
  .palette::backdrop {
    background: var(--scrim);
  }
  .field {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: 0 var(--sp-4);
    border-bottom: 1px solid var(--line);
    color: var(--ink-3);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 54px;
    border: 0;
    background: none;
    font-size: var(--fs-md);
    color: var(--ink);
  }
  input:focus {
    outline: none;
  }
  .searching {
    font-size: var(--fs-xs);
  }
  .list {
    max-height: min(420px, 60vh);
    overflow-y: auto;
    padding: var(--sp-2);
  }
  .group {
    padding: var(--sp-2) var(--sp-3) var(--sp-1);
    color: var(--ink-3);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: 40px;
    padding: 0 var(--sp-3);
    border-radius: var(--r-sm);
    cursor: pointer;
    color: var(--ink);
    font-size: var(--fs-sm);
  }
  .item :global(svg) {
    color: var(--ink-3);
  }
  .item[aria-selected='true'] {
    background: var(--accent-soft);
  }
  .item[aria-selected='true'] :global(svg) {
    color: var(--accent);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    margin-left: auto;
    color: var(--ink-3);
    font-size: var(--fs-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 45%;
  }
  .empty {
    padding: var(--sp-5) var(--sp-3);
    color: var(--ink-3);
    text-align: center;
  }
  .hints {
    display: flex;
    gap: var(--sp-4);
    padding: var(--sp-2) var(--sp-4);
    border-top: 1px solid var(--line);
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .hints kbd {
    margin-right: 2px;
  }
  @media (max-width: 640px) {
    .hints {
      display: none;
    }
  }
</style>
