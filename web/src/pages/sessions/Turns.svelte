<script lang="ts">
  import { queries } from '../../lib/api/queries';
  import { useQuery } from '../../lib/api/query.svelte';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import { describeError } from '../../lib/stores/app.svelte';
  import Answer from '../../lib/ui/Answer.svelte';
  import Banner from '../../lib/ui/Banner.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import EmptyState from '../../lib/ui/EmptyState.svelte';
  import Rich, { strong } from '../../lib/ui/Rich.svelte';
  import Skeleton from '../../lib/ui/Skeleton.svelte';
  import TurnTable from './TurnTable.svelte';

  interface Props {
    from: number;
    to: number;
    project?: number;
  }
  let { from, to, project }: Props = $props();
  const turns = useQuery(() => queries.turns({ from, to, project, limit: 25 }));
  const top = $derived(turns.data?.[0]);
</script>

{#if turns.error && !turns.data}
  <Banner tone="warning" title={t('common.error_title')} body={describeError(turns.error)}>
    {#snippet action()}<Button icon="refresh" onclick={() => turns.refetch()}>{t('common.retry')}</Button>{/snippet}
  </Banner>
{:else if !turns.data}
  <Card><Skeleton lines={8} height="1.4em" /></Card>
{:else if !turns.data.length}
  <Card><EmptyState icon="sessions" title={t('pages.sessions.turns_empty')} body={t('pages.sessions.empty_body')} /></Card>
{:else}
  <Answer confidence="lower_bound" refreshing={turns.refreshing}>
    {t('pages.sessions.turns_lede')}
    {#if top?.cost_usd != null}<Rich text={t('pages.sessions.turns_answer', { money: strong(fmt.money(top.cost_usd)), project: top.project })} />{/if}
  </Answer>
  <Card flush refreshing={turns.refreshing}>
    <TurnTable rows={turns.data} caption={t('pages.sessions.tab_turns')} />
  </Card>
{/if}
