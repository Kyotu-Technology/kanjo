<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t, tn } from '../i18n/index.svelte';
  import { href } from '../router';
  import { app } from '../stores/app.svelte';
  import { router } from '../stores/router.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import Tip from '../ui/Tip.svelte';
  import Brand from './Brand.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import LiveIndicator from './LiveIndicator.svelte';
  import { NAV_MAIN, NAV_SECONDARY, navFor, type NavItem } from './nav';
  import PrefSwitches from './PrefSwitches.svelte';

  interface Props {
    children: Snippet;
  }
  let { children }: Props = $props();
  let drawer = $state(false);
  const current = $derived(navFor(router.current.name)?.route);
  const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);

  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        app.paletteOpen = !app.paletteOpen;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  $effect(() => {
    void router.current;
    drawer = false;
  });

  const bellLabel = $derived(app.unread > 0 ? tn('shell.bell', app.unread) : t('shell.bell_none'));
</script>

{#snippet navList(items: NavItem[])}
  <ul>
    {#each items as item (item.route)}
      <li>
        <a href={href(item.path)} class:active={current === item.route} aria-current={current === item.route ? 'page' : undefined}>
          <Icon name={item.icon} />
          <span>{t(item.label)}</span>
          {#if item.route === 'alerts' && app.unread > 0}<span class="count" aria-hidden="true">{app.unread}</span>{/if}
        </a>
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet nav()}
  <nav aria-label={t('nav.label')}>
    {@render navList(NAV_MAIN)}
    <div class="sep" role="presentation"></div>
    {@render navList(NAV_SECONDARY)}
  </nav>
{/snippet}

<a class="skip" href="#main" onclick={(e) => { e.preventDefault(); document.getElementById('main')?.focus(); }}>{t('shell.skip')}</a>

<div class="shell">
  <aside class="sidebar">
    <a class="brand-link" href={href('/')} aria-label="Kanjo, {t('nav.dashboard')}"><Brand /></a>
    {@render nav()}
    <div class="side-foot">
      {#if app.mock}
        <Tip text={t('shell.mock_help')}><span class="mock"><Icon name="info" size={14} />{t('shell.mock')}</span></Tip>
      {/if}
      {#if app.demo}
        <Tip text={t('shell.demo_help')}><span class="mock"><Icon name="info" size={14} />{t('shell.demo')}</span></Tip>
      {/if}
      <p class="tagline">{t('nav.tagline')}</p>
    </div>
  </aside>

  <div class="main-col">
    <header class="topbar">
      <button type="button" class="menu-btn" aria-label={t('nav.open_menu')} aria-expanded={drawer} onclick={() => (drawer = true)}>
        <Icon name="menu" size={20} />
      </button>
      <a class="mobile-brand" href={href('/')} aria-label="Kanjo"><Brand /></a>
      <button type="button" class="search" onclick={() => (app.paletteOpen = true)} aria-keyshortcuts={isMac ? 'Meta+K' : 'Control+K'}>
        <Icon name="search" size={17} />
        <span class="search-text">{t('shell.search')}</span>
        <span class="search-short sr-only">{t('shell.search_short')}</span>
        <span class="keys" aria-hidden="true"><kbd>{isMac ? 'Cmd' : 'Ctrl'}</kbd><kbd>K</kbd></span>
      </button>
      <div class="spacer"></div>
      <div class="tools">
        {#if app.demo}
          <Tip text={t('shell.demo_help')}><span class="mock demo-pill"><Icon name="info" size={14} />{t('shell.demo')}</span></Tip>
        {/if}
        <span class="live-wrap"><LiveIndicator /></span>
        <span class="live-compact"><LiveIndicator compact /></span>
        <a class="bell" href={href('/alerts')} aria-label={bellLabel} title={bellLabel}>
          <Icon name="alerts" size={20} />
          {#if app.unread > 0}<span class="badge" aria-hidden="true">{app.unread > 99 ? '99+' : app.unread}</span>{/if}
        </a>
        <span class="prefs-wrap"><PrefSwitches /></span>
      </div>
    </header>

    <main id="main" tabindex="-1">
      {@render children()}
    </main>
  </div>
</div>

<Dialog bind:open={drawer} title={t('nav.label')} hideTitle placement="side" size="sm">
  <div class="drawer">
    <Brand />
    {@render nav()}
    <div class="drawer-prefs"><PrefSwitches /></div>
    {#if app.mock}<p class="mock-line">{t('shell.mock_help')}</p>{/if}
    {#if app.demo}<p class="mock-line">{t('shell.demo_help')}</p>{/if}
  </div>
</Dialog>

<CommandPalette />

<style>
  .skip {
    position: absolute;
    left: var(--sp-3);
    top: -60px;
    z-index: 100;
    padding: var(--sp-2) var(--sp-4);
    border-radius: var(--r-sm);
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: var(--fw-semibold);
  }
  .skip:focus {
    top: var(--sp-3);
  }
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-w) minmax(0, 1fr);
    min-height: 100vh;
  }
  .sidebar {
    position: sticky;
    top: 0;
    height: 100vh;
    display: flex;
    flex-direction: column;
    gap: var(--sp-6);
    padding: var(--sp-5) var(--sp-4);
    border-right: 1px solid var(--line);
    background: var(--surface);
    overflow-y: auto;
  }
  .brand-link {
    padding: 0 var(--sp-2);
    text-decoration: none;
    align-self: flex-start;
  }
  nav ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  nav a {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: 38px;
    padding: 0 var(--sp-3);
    border-radius: var(--r-sm);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    text-decoration: none;
  }
  nav a :global(svg) {
    color: var(--ink-3);
  }
  nav a:hover {
    background: var(--surface-2);
    color: var(--ink);
  }
  nav a.active {
    background: var(--accent-soft);
    color: var(--ink);
    font-weight: var(--fw-semibold);
  }
  nav a.active :global(svg) {
    color: var(--accent);
  }
  .count {
    margin-left: auto;
    min-width: 20px;
    padding: 0 6px;
    border-radius: var(--r-pill);
    background: var(--surface-3);
    color: var(--ink-2);
    font-size: var(--fs-xs);
    text-align: center;
    line-height: 20px;
  }
  .sep {
    height: 1px;
    margin: var(--sp-3) var(--sp-3);
    background: var(--line);
  }
  .side-foot {
    margin-top: auto;
    padding: 0 var(--sp-2);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--sp-3);
  }
  .tagline {
    color: var(--ink-3);
    font-size: var(--fs-xs);
    line-height: var(--lh-snug);
  }
  .mock {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    border-radius: var(--r-pill);
    background: var(--warning-soft);
    color: var(--warning-ink);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
  }
  .main-col {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .topbar {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    height: var(--topbar-h);
    padding: 0 var(--sp-7);
    background: color-mix(in srgb, var(--paper) 88%, transparent);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--line);
  }
  .menu-btn,
  .bell {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--ink-2);
  }
  .menu-btn:hover,
  .bell:hover {
    background: var(--surface-3);
    color: var(--ink);
  }
  .menu-btn,
  .mobile-brand {
    display: none;
  }
  .search {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    width: min(420px, 100%);
    min-height: 38px;
    padding: 0 var(--sp-2) 0 var(--sp-3);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--ink-3);
    font-size: var(--fs-sm);
    text-align: left;
  }
  .search:hover {
    border-color: var(--control-border);
    color: var(--ink-2);
  }
  .search-text {
    flex: 1;
  }
  .keys {
    display: inline-flex;
    gap: 3px;
  }
  .spacer {
    flex: 1;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .live-compact {
    display: none;
  }
  .badge {
    position: absolute;
    top: 3px;
    right: 2px;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    border-radius: var(--r-pill);
    border: 2px solid var(--paper);
    background: #c62828;
    color: #fff;
    font-size: 10px;
    font-weight: var(--fw-bold);
    line-height: 13px;
    text-align: center;
  }
  main {
    flex: 1;
    width: 100%;
    max-width: var(--content-max);
    padding: var(--sp-7) var(--sp-7) var(--sp-9);
  }
  main:focus {
    outline: none;
  }
  .drawer {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .drawer-prefs {
    padding-top: var(--sp-4);
    border-top: 1px solid var(--line);
  }
  .demo-pill {
    white-space: nowrap;
  }
  .mock-line {
    color: var(--warning-ink);
    font-size: var(--fs-sm);
  }

  @media (max-width: 1023px) {
    .shell {
      grid-template-columns: minmax(0, 1fr);
    }
    .sidebar {
      display: none;
    }
    .menu-btn {
      display: inline-grid;
      margin-left: calc(-1 * var(--sp-2));
    }
    .mobile-brand {
      display: inline-flex;
      text-decoration: none;
    }
    .topbar {
      padding: 0 var(--sp-5);
    }
    .search {
      width: auto;
      flex: 0 1 300px;
      margin-left: var(--sp-3);
    }
    main {
      padding: var(--sp-6) var(--sp-5) var(--sp-8);
    }
  }
  @media (max-width: 760px) {
    .prefs-wrap,
    .live-wrap {
      display: none;
    }
    .live-compact {
      display: inline;
    }
    .search {
      flex: none;
      width: 38px;
      padding: 0;
      justify-content: center;
      border-color: transparent;
      background: none;
      margin-left: 0;
    }
    .search-text,
    .keys {
      display: none;
    }
    .mobile-brand :global(.word) {
      font-size: 1.05rem;
    }
    .topbar {
      gap: var(--sp-1);
      padding: 0 var(--sp-4);
    }
    main {
      padding: var(--sp-5) var(--sp-4) var(--sp-8);
    }
  }
</style>
