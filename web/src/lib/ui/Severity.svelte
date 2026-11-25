<script lang="ts">
  import type { Severity } from '../api/contract';
  import { t } from '../i18n/index.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    severity: Severity;
    showLabel?: boolean;
  }
  let { severity, showLabel = false }: Props = $props();
</script>

<span class="sev {severity}" title={t(`pages.alerts.severity.${severity}`)}>
  <Icon name={severity === 'critical' ? 'critical' : severity === 'warning' ? 'warning' : 'info'} size={18} />
  <span class:sr-only={!showLabel}>{t(`pages.alerts.severity.${severity}`)}</span>
</span>

<style>
  .sev {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .info {
    color: var(--accent);
  }
  .warning {
    color: var(--warning-ink);
  }
  .critical {
    color: var(--critical-ink);
  }
</style>
