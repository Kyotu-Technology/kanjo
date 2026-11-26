<script lang="ts">
  import type { DoctorCheck } from '../../lib/api/contract';
  import { t } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';

  let { checks, refreshing = false }: { checks: DoctorCheck[]; refreshing?: boolean } = $props();
  const issues = $derived(checks.filter((c) => c.status !== 'ok'));
  const passed = $derived(checks.filter((c) => c.status === 'ok'));
  let showPassed = $state(false);
</script>

{#snippet row(c: DoctorCheck)}
  <li id="check-{c.id}" class={c.status}>
    <span class="state">
      <Icon name={c.status === 'ok' ? 'check' : c.status === 'warn' ? 'warning' : 'critical'} size={16} strokeWidth={c.status === 'ok' ? 2.4 : 2} />
      <span>{t(`system.checks.status.${c.status}`)}</span>
    </span>
    <div class="body">
      <p class="title">{c.title}</p>
      <p class="detail">{c.detail}</p>
      {#if c.fix}<p class="fix"><strong>{t('system.checks.fix')}:</strong> {c.fix}</p>{/if}
    </div>
  </li>
{/snippet}

<Card id="checks" title={t('system.checks.title')} subtitle={t('system.checks.subtitle')} {refreshing}>
  {#if issues.length}
    <ul class="issues">
      {#each issues as c (c.id)}{@render row(c)}{/each}
    </ul>
  {/if}
  <button type="button" class="toggle" aria-expanded={showPassed} aria-controls="checks-passed" onclick={() => (showPassed = !showPassed)}>
    <Icon name="check" size={16} strokeWidth={2.4} />
    <span>{issues.length ? t('system.checks.passed', { count: passed.length }) : t('system.checks.all_ok', { count: checks.length })}</span>
    <span class="more">{showPassed ? t('system.checks.hide') : t('system.checks.show')}</span>
    <Icon name={showPassed ? 'chevron_down' : 'chevron_right'} size={16} />
  </button>
  {#if showPassed}
    <ul class="passed" id="checks-passed">
      {#each passed as c (c.id)}{@render row(c)}{/each}
    </ul>
  {/if}
</Card>

<style>
  .issues {
    margin-bottom: var(--sp-3);
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    width: 100%;
    min-height: 40px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--good-ink);
    font-weight: var(--fw-medium);
    text-align: left;
  }
  .toggle:hover {
    border-color: var(--line-strong);
  }
  .more {
    margin-left: auto;
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .toggle > :global(svg:last-child) {
    color: var(--ink-3);
  }
  .passed {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    column-gap: var(--sp-6);
    margin-top: var(--sp-2);
  }
  li {
    display: grid;
    grid-template-columns: 136px minmax(0, 1fr);
    gap: var(--sp-3);
    padding: var(--sp-3) 0;
    border-top: 1px solid var(--line);
    scroll-margin-top: calc(var(--topbar-h) + var(--sp-6));
  }
  .issues li:first-child {
    border-top: 0;
    padding-top: 0;
  }
  .state {
    display: inline-flex;
    align-items: flex-start;
    gap: 6px;
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .state :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .ok .state {
    color: var(--good-ink);
  }
  .warn .state {
    color: var(--warning-ink);
  }
  .fail .state {
    color: var(--critical-ink);
  }
  .title {
    font-weight: var(--fw-semibold);
  }
  .detail {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    overflow-wrap: anywhere;
  }
  .fix {
    margin-top: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--ink);
    font-size: var(--fs-sm);
  }
  @media (max-width: 860px) {
    .passed {
      grid-template-columns: minmax(0, 1fr);
    }
    li {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-1);
    }
  }
</style>
