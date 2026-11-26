<script lang="ts">
  import type { BreakdownRow, Confidence } from '../../lib/api/contract';
  import ShareBars from '../../lib/charts/ShareBars.svelte';
  import { modelColor, modelLabel, modelTitle } from '../../lib/charts/series';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { href } from '../../lib/router';
  import Card from '../../lib/ui/Card.svelte';
  import ConfidenceBadge from '../../lib/ui/ConfidenceBadge.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Tabs from '../../lib/ui/Tabs.svelte';

  interface Props {
    projects: BreakdownRow[];
    models: BreakdownRow[];
    confidence: Confidence;
    refreshing?: boolean;
  }
  let { projects, models, confidence, refreshing = false }: Props = $props();
  let tab = $state<'projects' | 'models'>('projects');

  const rows = $derived(
    tab === 'projects'
      ? projects.map((r) => ({ key: r.key, label: r.label, value: fmt.money(r.usage.cost_usd), share: r.share, href: href('/projects', { id: r.key }) }))
      : models.map((r) => ({ key: r.key, label: modelLabel(r.key), title: modelTitle(r.key), value: fmt.money(r.usage.cost_usd), share: r.share, color: modelColor(r.key), href: href('/usage', { model: r.key }) })),
  );
</script>

<Card title={t('dashboard.top.title')} {refreshing}>
  {#snippet actions()}
    <ConfidenceBadge {confidence} />
  {/snippet}
  <p class="sub">{t('dashboard.top.subtitle')}</p>
  <Tabs
    label={t('dashboard.top.title')}
    size="sm"
    items={[
      { id: 'projects', label: t('dashboard.top.projects') },
      { id: 'models', label: t('dashboard.top.models') },
    ]}
    bind:value={tab}
  >
    {#if rows.length}
      <ShareBars {rows} label={t('dashboard.top.title')} shareFormat={(s) => fmt.ratio(s)} />
    {:else}
      <EmptyState compact icon="usage" title={t('dashboard.top.empty')} />
    {/if}
  </Tabs>
</Card>

<style>
  .sub {
    margin: -6px 0 var(--sp-3);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
