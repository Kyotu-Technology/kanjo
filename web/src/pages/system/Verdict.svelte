<script lang="ts">
  import type { SystemInfo } from '../../lib/api/contract';
  import { fmt, t, tn } from '../../lib/i18n/index.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import { address, isLoopback, lastActivity, scrollToSection } from './util';

  let { d, chainOk = true, lastRequestAt = null }: { d: SystemInfo; chainOk?: boolean; lastRequestAt?: number | null } = $props();
  const last = $derived(lastActivity(d.ingest.last_event_at, lastRequestAt));
  const fails = $derived([
    ...d.checks.filter((c) => c.status === 'fail'),
    ...(chainOk ? [] : [{ id: 'audit', status: 'fail' as const, title: t('system.audit.chain_title'), detail: '', fix: null }]),
  ]);
  const warns = $derived(d.checks.filter((c) => c.status === 'warn'));
  const level = $derived(fails.length ? 'fail' : warns.length ? 'warn' : 'ok');
  const addr = $derived(address(d.server.bind, d.server.port));
  const headline = $derived(level === 'fail' ? tn('system.verdict.fail', fails.length) : level === 'warn' ? tn('system.verdict.warn', warns.length) : t('system.verdict.ok'));
  const issues = $derived([...fails, ...warns]);
</script>

<div class="top">
  <section class="verdict {level}" aria-labelledby="verdict-h">
    <span class="glyph" aria-hidden="true"><Icon name={level === 'ok' ? 'check' : level === 'warn' ? 'warning' : 'critical'} size={26} strokeWidth={2} /></span>
    <div class="text">
      <h2 id="verdict-h">{headline}</h2>
      <p>
        {d.claude_code_version
          ? t('system.verdict.body', { version: d.version, address: addr, cc: d.claude_code_version })
          : t('system.verdict.body_no_cc', { version: d.version, address: addr })}
        {last ? t('system.verdict.last', { ago: fmt.ago(last) }) : t('system.verdict.none_yet')}
      </p>
      {#if issues.length}
        <ul class="issues">
          {#each issues as c (c.id)}
            <li>
              <button type="button" onclick={() => scrollToSection(c.id === 'audit' && !chainOk ? 'audit' : `check-${c.id}`)}>
                <Icon name={c.status === 'fail' ? 'critical' : 'warning'} size={15} />{c.title}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </section>

  <section class="never" aria-labelledby="never-h">
    <h2 id="never-h">{t('system.never.title')}</h2>
    <ul>
      <li><Icon name="globe" size={17} /><span>{t('system.never.telemetry')}</span></li>
      <li><Icon name="lock" size={17} /><span>{t('system.never.text')}</span></li>
      <li><Icon name="settings" size={17} /><span>{t('system.never.settings')}</span></li>
      {#if isLoopback(d.server.bind)}
        <li><Icon name="monitor" size={17} /><span>{t('system.never.local', { address: addr })}</span></li>
      {:else}
        <li class="exposed"><Icon name="warning" size={17} /><span>{t('system.never.exposed', { address: addr })}</span></li>
      {/if}
    </ul>
  </section>
</div>

<style>
  .top {
    display: grid;
    grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr);
    gap: var(--sp-5);
  }
  .verdict {
    display: flex;
    align-items: center;
    gap: var(--sp-4);
    padding: var(--sp-6);
    border-radius: var(--r-lg);
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .verdict.ok {
    background: var(--good-soft);
    border-color: transparent;
  }
  .verdict.warn {
    background: var(--warning-soft);
    border-color: transparent;
  }
  .verdict.fail {
    background: var(--critical-soft);
    border-color: transparent;
  }
  .glyph {
    display: grid;
    place-items: center;
    flex: none;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: var(--surface);
  }
  .ok .glyph {
    color: var(--good-ink);
  }
  .warn .glyph {
    color: var(--warning-ink);
  }
  .fail .glyph {
    color: var(--critical-ink);
  }
  h2 {
    font-size: var(--fs-xl);
  }
  .text p {
    margin-top: var(--sp-2);
    color: var(--ink-2);
    max-width: 60ch;
  }
  .issues {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
    margin-top: var(--sp-4);
  }
  .issues button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    padding: 0 var(--sp-3);
    border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
    border-radius: var(--r-pill);
    background: var(--surface);
    color: var(--ink);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .issues button:hover {
    border-color: var(--control-border);
  }
  .warn .issues :global(svg) {
    color: var(--warning-ink);
  }
  .fail .issues :global(svg) {
    color: var(--critical-ink);
  }
  .never {
    padding: var(--sp-5) var(--sp-6);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--surface);
  }
  .never h2 {
    font-size: var(--fs-md);
  }
  .never ul {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    margin-top: var(--sp-3);
  }
  .never li {
    display: flex;
    gap: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    line-height: var(--lh-snug);
  }
  .never li :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--accent);
  }
  .never li.exposed :global(svg) {
    color: var(--warning-ink);
  }
  @media (max-width: 980px) {
    .top {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 560px) {
    .verdict {
      flex-direction: column;
      padding: var(--sp-5);
    }
    .never {
      padding: var(--sp-5);
    }
  }
</style>
