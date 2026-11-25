<script lang="ts">
  import { t } from '../lib/i18n/index.svelte';
  import { href } from '../lib/router';
  import Button from '../lib/ui/Button.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
</script>

<EmptyState icon="info" title={t('common.error_not_found')}>
  {#snippet action()}<Button href={href('/')} variant="primary">{t('nav.dashboard')}</Button>{/snippet}
</EmptyState>
