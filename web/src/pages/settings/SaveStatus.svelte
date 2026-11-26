<script lang="ts">
  import { t } from '../../lib/i18n/index.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import type { SaveState } from './saver.svelte';

  interface Props {
    state: SaveState;
  }
  let { state }: Props = $props();
</script>

<span class="save {state}" role="status" aria-live="polite">
  {#if state === 'saving'}
    <span class="spin" aria-hidden="true"></span>{t('settings.status.saving')}
  {:else if state === 'saved'}
    <Icon name="check" size={15} strokeWidth={2.4} />{t('settings.status.saved')}
  {:else if state === 'failed'}
    <Icon name="warning" size={15} />{t('settings.status.failed')}
  {/if}
</span>

<style>
  .save {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 24px;
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    white-space: nowrap;
  }
  .saving {
    color: var(--ink-3);
  }
  .saved {
    color: var(--good-ink);
  }
  .failed {
    color: var(--critical-ink);
    white-space: normal;
  }
  .spin {
    width: 12px;
    height: 12px;
    border: 2px solid var(--line-strong);
    border-top-color: var(--ink-2);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
