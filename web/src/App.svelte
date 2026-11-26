<script lang="ts">
  import type { Component } from 'svelte';
  import { t } from './lib/i18n/index.svelte';
  import type { RouteName } from './lib/router';
  import { app, bootstrap } from './lib/stores/app.svelte';
  import { router } from './lib/stores/router.svelte';
  import AuthScreen from './lib/shell/AuthScreen.svelte';
  import Onboarding from './lib/shell/Onboarding.svelte';
  import Shell from './lib/shell/Shell.svelte';
  import Brand from './lib/shell/Brand.svelte';
  import Button from './lib/ui/Button.svelte';
  import Toaster from './lib/ui/Toaster.svelte';
  import Dashboard from './pages/Dashboard.svelte';
  import Extras from './pages/help/Extras.svelte';

  type Loader = () => Promise<{ default: Component }>;
  const LAZY: Partial<Record<RouteName, Loader>> = {
    usage: () => import('./pages/Usage.svelte'),
    sessions: () => import('./pages/Sessions.svelte'),
    session: () => import('./pages/Session.svelte'),
    limits: () => import('./pages/Limits.svelte'),
    insights: () => import('./pages/Insights.svelte'),
    alerts: () => import('./pages/Alerts.svelte'),
    projects: () => import('./pages/Projects.svelte'),
    settings: () => import('./pages/Settings.svelte'),
    system: () => import('./pages/System.svelte'),
    not_found: () => import('./pages/NotFound.svelte'),
  };

  let loaded = $state<Partial<Record<RouteName, Component>>>({});
  const name = $derived(router.current.name);
  const Page = $derived(name === 'dashboard' ? (Dashboard as Component) : loaded[name]);

  $effect(() => {
    const n = name;
    const loader = LAZY[n];
    if (loader && !loaded[n])
      loader().then((m) => {
        loaded = { ...loaded, [n]: m.default };
      });
  });

  $effect(() => {
    void bootstrap();
  });

  $effect(() => {
    if (app.boot === 'ready') document.title = `Kanjo${app.mock ? ` (${t('shell.mock')})` : ''}`;
  });
</script>

{#if app.boot === 'loading'}
  <div class="splash" aria-busy="true"><Brand /></div>
{:else if app.auth === 'unauthorized'}
  <AuthScreen />
{:else if app.boot === 'failed'}
  <div class="splash failed" role="alert">
    <Brand />
    <h1>{t('common.error_title')}</h1>
    <p>{app.bootError}</p>
    <Button variant="primary" icon="refresh" onclick={() => location.reload()}>{t('common.retry')}</Button>
  </div>
{:else if app.settings && !app.settings.onboarding_done}
  <Onboarding />
{:else}
  <Shell>
    {#key name + JSON.stringify(router.current.params)}
      {#if Page}<Page />{/if}
    {/key}
  </Shell>
{/if}
<Toaster />
<Extras />

<style>
  .splash {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--sp-4);
    padding: var(--sp-6);
    text-align: center;
  }
  .splash :global(.brand) {
    opacity: 0.85;
  }
  .failed h1 {
    font-size: var(--fs-xl);
  }
  .failed p {
    color: var(--ink-2);
    max-width: 52ch;
  }
</style>
