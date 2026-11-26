<script lang="ts">
  import { t } from '../lib/i18n/index.svelte';
  import type { Route } from '../lib/router';
  import { app } from '../lib/stores/app.svelte';
  import { router } from '../lib/stores/router.svelte';
  import Card from '../lib/ui/Card.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';
  import AlertsSection from './settings/AlertsSection.svelte';
  import CurrencySection from './settings/CurrencySection.svelte';
  import './settings/form.css';
  import LookSection from './settings/LookSection.svelte';
  import PlanSection from './settings/PlanSection.svelte';
  import PrivacySection from './settings/PrivacySection.svelte';
  import RetentionSection from './settings/RetentionSection.svelte';
  import SectionNav from './settings/SectionNav.svelte';
  import { goToSection } from './settings/saver.svelte';
  import Summary from './settings/Summary.svelte';

  const s = $derived(app.settings);
  const sections = $derived([
    { id: 'look', label: t('settings.look.title') },
    { id: 'plan', label: t('settings.plan.title') },
    { id: 'currency', label: t('settings.currency.title') },
    { id: 'alerts', label: t('settings.alerts.title') },
    { id: 'privacy', label: t('settings.privacy.title') },
    { id: 'retention', label: t('settings.retention.title') },
  ]);

  let jumped: Route | null = null;
  $effect(() => {
    const r = router.current;
    if (!app.settings || !r.query.section || r === jumped) return;
    jumped = r;
    const id = setTimeout(() => goToSection(r.query.section), 60);
    return () => clearTimeout(id);
  });
</script>

<PageHeader title={t('settings.title')} lede={t('settings.lede')} />

{#if s}
  <div class="layout kj-form">
    <aside class="side"><SectionNav label={t('settings.sections')} items={sections} /></aside>
    <div class="stack">
      <Summary {s} />
      <LookSection {s} />
      <PlanSection {s} />
      <CurrencySection {s} />
      <AlertsSection {s} />
      <PrivacySection {s} />
      <RetentionSection {s} />
    </div>
  </div>
{:else}
  <div class="stack" aria-busy="true">
    {#each [0, 1, 2] as i (i)}<Card><Skeleton lines={4} /></Card>{/each}
  </div>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: 200px minmax(0, 820px);
    gap: var(--sp-8);
    align-items: start;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
    min-width: 0;
  }
  .stack > :global(.card) {
    scroll-margin-top: calc(var(--topbar-h) + var(--sp-4));
  }
  @media (max-width: 1180px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    .side {
      display: none;
    }
    .stack {
      max-width: 820px;
    }
  }
</style>
