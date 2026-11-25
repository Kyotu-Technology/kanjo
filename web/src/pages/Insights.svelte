<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import ShareBars from '../lib/charts/ShareBars.svelte';
  import { modelLabel } from '../lib/charts/series';
  import { fmt, t } from '../lib/i18n/index.svelte';
  import Card from '../lib/ui/Card.svelte';
  import EmptyState from '../lib/ui/EmptyState.svelte';
  import InlineHelp from '../lib/ui/InlineHelp.svelte';
  import Money from '../lib/ui/Money.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';

  const insights = useQuery(() => queries.insights());
  const d = $derived(insights.data);
</script>

<PageHeader title={t('pages.insights.title')} lede={t('pages.insights.lede')} />

{#if d}
  <div class="grid">
    <Card title={t('pages.insights.cache')}>
      {#snippet actions()}<InlineHelp text={t('pages.insights.cache_help')} />{/snippet}
      <p class="big">{d.cache.hit_ratio === null ? t('common.unknown') : fmt.ratio(d.cache.hit_ratio)}</p>
      <ShareBars
        label={t('pages.insights.cache')}
        shareFormat={() => ''}
        rows={d.cache.by_project.map((r) => ({ key: r.key, label: r.label, value: r.hit_ratio === null ? '' : fmt.ratio(r.hit_ratio), share: r.hit_ratio ?? 0 }))}
      />
    </Card>
    <Card title={t('pages.insights.suggestions')}>
      {#if d.model_mix.length}
        <ul class="sugg">
          {#each d.model_mix as m (m.code)}
            <li>
              <p class="title">{modelLabel(m.from_model)}, {modelLabel(m.to_model)}</p>
              <p class="saving">{t('pages.insights.saving', { value: '' })}<Money usd={m.saving_usd} confidence={m.confidence} size="sm" badge="seal" /></p>
            </li>
          {/each}
        </ul>
      {:else}
        <EmptyState compact title={t('pages.insights.no_suggestions')} />
      {/if}
    </Card>
  </div>
  <p class="preview">{t('pages.preview')}</p>
{:else}
  <Card><Skeleton lines={5} /></Card>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--sp-5);
  }
  .big {
    font-size: var(--fs-2xl);
    font-weight: var(--fw-semibold);
    margin-bottom: var(--sp-4);
  }
  .sugg li + li {
    margin-top: var(--sp-3);
  }
  .title {
    font-weight: var(--fw-semibold);
  }
  .saving {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .preview {
    margin-top: var(--sp-5);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
