<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import { retryAuth } from '../stores/app.svelte';
  import { toasts } from '../stores/toast.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import Brand from './Brand.svelte';
  import PrefSwitches from './PrefSwitches.svelte';

  async function copy() {
    try {
      await navigator.clipboard.writeText('kanjo open');
      toasts.push('success', t('common.copied'));
    } catch {
      // clipboard can be blocked; the command stays visible and selectable
    }
  }
</script>

<div class="auth">
  <div class="top"><Brand /><PrefSwitches /></div>
  <main class="panel">
    <span class="glyph"><Icon name="lock" size={22} /></span>
    <h1>{t('auth.title')}</h1>
    <p>{t('auth.body')}</p>
    <div class="cmd">
      <code><span class="prompt" aria-hidden="true">$</span>kanjo open</code>
      <Button size="sm" variant="ghost" onclick={copy}>{t('common.copy')}</Button>
    </div>
    <p class="why">{t('auth.why')}</p>
    <Button variant="primary" icon="refresh" onclick={() => retryAuth()}>{t('auth.retry')}</Button>
  </main>
</div>

<style>
  .auth {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    padding: var(--sp-5);
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    max-width: 960px;
    width: 100%;
    margin: 0 auto;
  }
  .panel {
    margin: auto;
    width: min(520px, 100%);
    padding: var(--sp-8) var(--sp-7);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--surface);
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
  }
  h1 {
    margin-top: var(--sp-5);
    font-size: var(--fs-2xl);
  }
  p {
    margin-top: var(--sp-3);
    color: var(--ink-2);
  }
  .cmd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    margin: var(--sp-5) 0;
    padding: var(--sp-3) var(--sp-3) var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    background: var(--ink);
    color: var(--ink-inverse);
  }
  .cmd code {
    font-size: var(--fs-md);
  }
  .prompt {
    margin-right: 0.6em;
    opacity: 0.6;
  }
  .cmd :global(.btn) {
    --btn-ink: var(--ink-inverse);
  }
  .cmd :global(.btn:hover) {
    background: color-mix(in srgb, var(--ink-inverse) 14%, transparent) !important;
    color: var(--ink-inverse) !important;
  }
  .why {
    font-size: var(--fs-sm);
    margin-bottom: var(--sp-5);
  }
  @media (max-width: 560px) {
    .panel {
      padding: var(--sp-6) var(--sp-5);
    }
  }
</style>
