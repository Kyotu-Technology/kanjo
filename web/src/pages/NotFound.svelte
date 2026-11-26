<script lang="ts">
  import { t } from '../lib/i18n/index.svelte';
  import { href } from '../lib/router';
  import { NAV_ALL } from '../lib/shell/nav';
  import { app } from '../lib/stores/app.svelte';
  import { router } from '../lib/stores/router.svelte';
  import Button from '../lib/ui/Button.svelte';
  import Icon from '../lib/ui/Icon.svelte';

  const path = $derived(router.current.path || '/');
  const pages = NAV_ALL.filter((n) => n.route !== 'dashboard');
</script>

<section class="nf" aria-labelledby="nf-h">
  <p class="code" aria-hidden="true">{t('not_found.code')}</p>
  <h1 id="nf-h">{t('not_found.title')}</h1>
  <p class="body">{t('not_found.body', { path: '#' + path })}</p>
  <div class="actions">
    <Button href={href('/')} variant="primary" icon="dashboard">{t('not_found.home')}</Button>
    <Button icon="search" onclick={() => (app.paletteOpen = true)}>{t('not_found.search')}</Button>
  </div>
  <h2 class="try">{t('not_found.try')}</h2>
  <ul class="pages">
    {#each pages as p (p.route)}
      <li><a href={href(p.path)}><Icon name={p.icon} size={17} /><span>{t(p.label)}</span></a></li>
    {/each}
  </ul>
</section>

<style>
  .nf {
    max-width: 640px;
    margin: var(--sp-8) auto 0;
    text-align: left;
  }
  .code {
    color: transparent;
    -webkit-text-stroke: 1.5px var(--line-strong);
    font-size: clamp(5rem, 18vw, 9rem);
    font-weight: var(--fw-bold);
    line-height: 0.9;
    letter-spacing: -0.04em;
    font-variant-numeric: tabular-nums;
  }
  h1 {
    margin-top: var(--sp-4);
    font-size: var(--fs-2xl);
  }
  .body {
    margin-top: var(--sp-3);
    color: var(--ink-2);
    max-width: 56ch;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-3);
    margin-top: var(--sp-6);
  }
  .try {
    margin-top: var(--sp-8);
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .pages {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: var(--sp-2);
    margin-top: var(--sp-3);
  }
  .pages a {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: 44px;
    padding: 0 var(--sp-4);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--ink);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    text-decoration: none;
  }
  .pages a :global(svg) {
    color: var(--ink-3);
  }
  .pages a:hover {
    border-color: var(--line-strong);
    background: var(--surface-2);
  }
  .pages a:hover :global(svg) {
    color: var(--accent);
  }
</style>
