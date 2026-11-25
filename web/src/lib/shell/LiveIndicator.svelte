<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import { app } from '../stores/app.svelte';
  import Tip from '../ui/Tip.svelte';

  interface Props {
    compact?: boolean;
  }
  let { compact = false }: Props = $props();
  const label = $derived(t(`shell.live.${app.live}`));
  const help = $derived(app.live === 'open' ? t('shell.live.help_open') : t('shell.live.help_retry'));
</script>

<Tip text={help} label={compact ? label : undefined}>
  <span class="live {app.live}">
    <span class="dot" aria-hidden="true"></span>
    <span class:sr-only={compact}>{label}</span>
  </span>
</Tip>

<style>
  .live {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 32px;
    padding: 0 var(--sp-2);
    border-radius: var(--r-sm);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ink-3);
  }
  .open .dot {
    background: var(--good);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--good) 22%, transparent);
  }
  .connecting .dot,
  .reconnecting .dot {
    background: var(--warning);
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
