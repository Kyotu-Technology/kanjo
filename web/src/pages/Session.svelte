<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import ShareBars from '../lib/charts/ShareBars.svelte';
  import { modelColor, modelLabel } from '../lib/charts/series';
  import { fmt, t } from '../lib/i18n/index.svelte';
  import { href } from '../lib/router';
  import { describeError } from '../lib/stores/app.svelte';
  import { router } from '../lib/stores/router.svelte';
  import Banner from '../lib/ui/Banner.svelte';
  import Card from '../lib/ui/Card.svelte';
  import Money from '../lib/ui/Money.svelte';
  import ConfidenceBadge from '../lib/ui/ConfidenceBadge.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';

  const id = $derived(router.current.params.id ?? '');
  const detail = useQuery(() => queries.session(id));
  const s = $derived(detail.data?.summary);
</script>

<p class="back"><a href={href('/sessions')}>{t('pages.session.back')}</a></p>
<PageHeader title={s?.title ?? t('pages.session.title')} lede={s ? `${s.project}${s.git_branch ? `, ${s.git_branch}` : ''}` : undefined} />

{#if detail.error}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(detail.error)} />
{:else if detail.data && s}
  <div class="grid">
    <Card title={t('dashboard.sessions.cost')}>
      <Money usd={s.usage.cost_usd} confidence="lower_bound" unpriced={s.usage.unpriced_requests} size="lg" />
      <p class="meta">{fmt.dateTime(s.started_at)}, {fmt.duration(s.last_at - s.started_at)}</p>
    </Card>
    <Card title={t('dashboard.block.by_model')}>
      {#snippet actions()}<ConfidenceBadge confidence="lower_bound" />{/snippet}
      <ShareBars
        label={t('dashboard.block.by_model')}
        shareFormat={(x) => fmt.ratio(x)}
        rows={detail.data.by_model.map((r) => ({ key: r.key, label: modelLabel(r.key), value: fmt.money(r.usage.cost_usd), share: r.share, color: modelColor(r.key) }))}
      />
    </Card>
  </div>
  <p class="preview">{t('pages.preview')}</p>
{:else}
  <Card><Skeleton lines={4} /></Card>
{/if}

<style>
  .back {
    margin-bottom: var(--sp-3);
    font-size: var(--fs-sm);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: var(--sp-5);
  }
  .meta {
    margin-top: var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .preview {
    margin-top: var(--sp-5);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
