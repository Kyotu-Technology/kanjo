<script lang="ts">
  import { fmt, i18n, t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/stores/app.svelte';
  import { router } from '../../lib/stores/router.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';
  import './print.css';

  const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
  const mod = isMac ? 'Cmd' : 'Ctrl';
  let open = $state(false);

  function typing(el: EventTarget | null): boolean {
    if (!(el instanceof HTMLElement)) return false;
    return el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName);
  }

  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '?' || e.ctrlKey || e.metaKey || e.altKey || typing(e.target)) return;
      if (app.boot !== 'ready' || app.auth !== 'ok' || app.paletteOpen) return;
      if (!open && document.querySelector('dialog[open]')) return;
      e.preventDefault();
      open = !open;
    };
    const onAsk = () => (open = true);
    window.addEventListener('keydown', onKey);
    window.addEventListener('kanjo:shortcuts', onAsk);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('kanjo:shortcuts', onAsk);
    };
  });

  function stampPrint() {
    const main = document.getElementById('main');
    main?.setAttribute('data-print-head', t('print.header', { date: fmt.dateTime(Date.now()) }));
    main?.setAttribute('data-print-foot', t('print.footer'));
  }

  $effect(() => {
    void router.current;
    void i18n.lang;
    if (app.boot !== 'ready') return;
    const id = setTimeout(stampPrint, 0);
    return () => clearTimeout(id);
  });

  /** Keeps the dated print header current; the light print palette comes from tokens.css. */
  $effect(() => {
    const stamp = () => stampPrint();
    const timer = setInterval(stamp, 60_000);
    window.addEventListener('beforeprint', stamp);
    return () => {
      clearInterval(timer);
      window.removeEventListener('beforeprint', stamp);
    };
  });

  const general = $derived([
    { keys: [mod, 'K'], text: t('shortcuts.palette') },
    { keys: ['?'], text: t('shortcuts.help') },
    { keys: [t('shortcuts.keys_tab')], text: t('shortcuts.tab') },
    { keys: [t('shortcuts.keys_enter')], text: t('shortcuts.activate') },
    { keys: [t('shortcuts.keys_escape')], text: t('shortcuts.escape') },
    { keys: [t('shortcuts.keys_arrows')], text: t('shortcuts.arrows') },
  ]);
  const palette = $derived([
    { keys: [t('shortcuts.keys_updown')], text: t('shortcuts.palette_move') },
    { keys: [t('shortcuts.keys_return')], text: t('shortcuts.palette_open') },
    { keys: [t('shortcuts.keys_escape')], text: t('shortcuts.escape') },
  ]);
</script>

{#snippet group(title: string, rows: { keys: string[]; text: string }[])}
  <h3>{title}</h3>
  <dl>
    {#each rows as r (r.text + r.keys.join())}
      <div>
        <dt>{#each r.keys as k, i (i)}{#if i > 0}<span class="plus" aria-hidden="true">+</span>{/if}<kbd>{k}</kbd>{/each}</dt>
        <dd>{r.text}</dd>
      </div>
    {/each}
  </dl>
{/snippet}

<Dialog bind:open title={t('shortcuts.title')} description={t('shortcuts.intro')} size="md">
  {@render group(t('shortcuts.general'), general)}
  {@render group(t('shortcuts.in_palette'), palette)}
  <p class="skip">{t('shortcuts.skip')}</p>
</Dialog>

<style>
  h3 {
    margin: var(--sp-4) 0 var(--sp-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  h3:first-child {
    margin-top: 0;
  }
  dl > div {
    display: grid;
    grid-template-columns: 150px minmax(0, 1fr);
    gap: var(--sp-3);
    padding: var(--sp-2) 0;
    border-top: 1px solid var(--line);
  }
  dt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }
  .plus {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  dd {
    color: var(--ink);
    font-size: var(--fs-sm);
  }
  .skip {
    margin-top: var(--sp-4);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  @media (max-width: 480px) {
    dl > div {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-1);
    }
  }
</style>
