<script lang="ts">
  import type { PlanKind, PrivacySettings } from '../api/contract';
  import { queries } from '../api/queries';
  import { useQuery } from '../api/query.svelte';
  import { fmt, i18n, t, tk, type Lang } from '../i18n/index.svelte';
  import { app, setLanguage, updateSettings } from '../stores/app.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import Skeleton from '../ui/Skeleton.svelte';
  import Switch from '../ui/Switch.svelte';
  import Brand from './Brand.svelte';
  import ConnectPanel from './ConnectPanel.svelte';
  import PrefSwitches from './PrefSwitches.svelte';

  const STEPS = ['language', 'found', 'privacy', 'connect', 'plan', 'done'] as const;
  type Step = (typeof STEPS)[number];
  const PLANS: { kind: PlanKind; price: number | null; seat?: boolean }[] = [
    { kind: 'pro', price: 20 },
    { kind: 'max5x', price: 100 },
    { kind: 'max20x', price: 200 },
    { kind: 'team', price: 30, seat: true },
    { kind: 'team_premium', price: 150, seat: true },
    { kind: 'enterprise', price: null },
    { kind: 'api', price: null },
    { kind: 'unknown', price: null },
  ];

  const initial = Number(new URLSearchParams(location.search).get('step'));
  let index = $state(Number.isInteger(initial) && initial >= 1 && initial <= STEPS.length ? initial - 1 : 0);
  const step = $derived<Step>(STEPS[index]);
  let privacy = $state<PrivacySettings>({ ...(app.settings?.privacy ?? { session_titles: false, redact_paths: false, conversation_viewer: false }) });
  let plan = $state<PlanKind>(app.settings?.plan.kind ?? 'unknown');
  let price = $state(app.settings?.plan.monthly_usd != null ? String(app.settings.plan.monthly_usd) : '');
  let connected = $state(false);
  let saving = $state(false);
  let heading: HTMLHeadingElement | undefined = $state();

  const system = useQuery(() => queries.system());
  const ingest = $derived(app.ingest ?? system.data?.ingest ?? null);
  const progress = $derived(ingest && ingest.files_total > 0 ? ingest.files_done / ingest.files_total : 0);

  async function next() {
    saving = true;
    try {
      if (step === 'privacy') await updateSettings((s) => ({ ...s, privacy: { ...privacy } }));
      if (step === 'plan') {
        const usd = Number(price.replace(',', '.'));
        await updateSettings((s) => ({ ...s, plan: { kind: plan, monthly_usd: price.trim() && Number.isFinite(usd) && usd > 0 ? usd : null } }));
      }
      if (step === 'done') {
        await updateSettings((s) => ({ ...s, onboarding_done: true }));
        return;
      }
      index = Math.min(STEPS.length - 1, index + 1);
      queueMicrotask(() => heading?.focus());
    } finally {
      saving = false;
    }
  }

  function back() {
    index = Math.max(0, index - 1);
    queueMicrotask(() => heading?.focus());
  }

  const enabledPrivacy = $derived(
    [privacy.session_titles && t('onboarding.privacy.titles'), privacy.redact_paths && t('onboarding.privacy.redact'), privacy.conversation_viewer && t('onboarding.privacy.viewer')]
      .filter(Boolean)
      .join(', ') || t('common.none'),
  );
  const hooksOn = $derived(connected || !!(system.data?.hooks.statusline_installed && system.data.hooks.session_hooks_installed));
  const LANGS: Lang[] = ['en', 'pl'];
</script>

<div class="onb">
  <header class="top">
    <Brand />
    <PrefSwitches />
  </header>

  <div class="frame">
    <nav class="rail" aria-label={t('onboarding.label')}>
      <ol>
        {#each STEPS as s, i (s)}
          <li class:current={i === index} class:done={i < index} aria-current={i === index ? 'step' : undefined}>
            <span class="num" aria-hidden="true">{#if i < index}<Icon name="check" size={14} strokeWidth={2.6} />{:else}{i + 1}{/if}</span>
            <span class="lbl">{t(`onboarding.steps.${s}`)}</span>
          </li>
        {/each}
      </ol>
    </nav>

    <main class="panel">
      <p class="count">{t('onboarding.step', { n: index + 1, total: STEPS.length })}</p>
      <div class="bar" aria-hidden="true"><span style="width:{((index + 1) / STEPS.length) * 100}%"></span></div>

      {#if step === 'language'}
        <h1 bind:this={heading} tabindex="-1">{t('onboarding.language.title')}</h1>
        <p class="lede">{t('onboarding.language.body')}</p>
        <fieldset class="choices">
          <legend>{t('onboarding.language.choose')}</legend>
          {#each LANGS as l (l)}
            <label class="choice" class:selected={i18n.lang === l}>
              <input type="radio" name="lang" value={l} checked={i18n.lang === l} onchange={() => setLanguage(l)} />
              <span class="choice-title" lang={l}>{t(`lang.${l}`)}</span>
              {#if i18n.lang === l}<Icon name="check" size={18} strokeWidth={2.4} class="tick" />{/if}
            </label>
          {/each}
        </fieldset>
      {:else if step === 'found'}
        <h1 bind:this={heading} tabindex="-1">{t('onboarding.found.title')}</h1>
        {#if system.data}
          <dl class="facts">
            <div><dt>{t('onboarding.found.folder')}</dt><dd><code>{system.data.claude_dir}</code></dd></div>
            <div><dt>{t('onboarding.found.version')}</dt><dd>{system.data.claude_code_version ?? t('onboarding.found.version_unknown')}</dd></div>
            <div><dt>{t('onboarding.found.files')}</dt><dd>{fmt.number(ingest?.files_total ?? 0)}</dd></div>
            <div><dt>{t('onboarding.found.requests')}</dt><dd>{fmt.number(ingest?.requests_stored ?? 0)}</dd></div>
          </dl>
          {#if ingest && ingest.files_total > 0}
            <div class="import">
              <p class="import-title">
                {#if ingest.backfill_running}<Icon name="refresh" size={16} class="spin" />{t('onboarding.found.importing')}{:else}<Icon name="check" size={16} strokeWidth={2.4} />{t('onboarding.found.imported')}{/if}
                <span class="import-num">{t('onboarding.found.progress', { done: fmt.number(ingest.files_done), total: fmt.number(ingest.files_total) })}</span>
              </p>
              <div class="pbar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(progress * 100)} aria-label={t('onboarding.found.importing')}>
                <span style="width:{Math.max(2, progress * 100)}%"></span>
              </div>
              {#if ingest.backfill_running}<p class="hint">{t('onboarding.found.keep_going')}</p>{/if}
            </div>
          {:else}
            <p class="lede">{t('onboarding.found.nothing')}</p>
          {/if}
        {:else}
          <Skeleton lines={4} height="1.2em" />
        {/if}
      {:else if step === 'privacy'}
        <h1 bind:this={heading} tabindex="-1">{t('onboarding.privacy.title')}</h1>
        <p class="lede">{t('onboarding.privacy.body')}</p>
        <div class="switches">
          <Switch bind:checked={privacy.session_titles} label={t('onboarding.privacy.titles')} description={t('onboarding.privacy.titles_help')} />
          <Switch bind:checked={privacy.redact_paths} label={t('onboarding.privacy.redact')} description={t('onboarding.privacy.redact_help')} />
          <Switch bind:checked={privacy.conversation_viewer} label={t('onboarding.privacy.viewer')} description={t('onboarding.privacy.viewer_help')} />
        </div>
      {:else if step === 'connect'}
        <h1 bind:this={heading} tabindex="-1">{t('onboarding.connect.title')}</h1>
        <p class="lede">{t('onboarding.connect.body')}</p>
        <div class="connect"><ConnectPanel onconnected={() => (connected = true)} /></div>
      {:else if step === 'plan'}
        <h1 bind:this={heading} tabindex="-1">{t('onboarding.plan.title')}</h1>
        <p class="lede">{t('onboarding.plan.body')}</p>
        <fieldset class="plans">
          <legend class="sr-only">{t('onboarding.plan.title')}</legend>
          {#each PLANS as p (p.kind)}
            <label class="choice plan" class:selected={plan === p.kind}>
              <input type="radio" name="plan" value={p.kind} checked={plan === p.kind} onchange={() => (plan = p.kind)} />
              <span class="choice-title">{tk(`plan.${p.kind}`)}</span>
              <span class="choice-sub">
                {#if p.price !== null}
                  {t(p.seat ? 'onboarding.plan.per_seat' : 'onboarding.plan.per_month', { price: fmt.money(p.price, { digits: 0 }) })}
                {:else if p.kind !== 'unknown'}
                  {t('onboarding.plan.no_price')}
                {/if}
              </span>
            </label>
          {/each}
        </fieldset>
        {#if plan !== 'unknown' && plan !== 'api'}
          <label class="price">
            <span>{t('onboarding.plan.price')}</span>
            <input type="text" inputmode="decimal" bind:value={price} placeholder={String(PLANS.find((p) => p.kind === plan)?.price ?? '')} />
            <span class="hint">{t('onboarding.plan.price_help')}</span>
          </label>
        {/if}
      {:else}
        <h1 bind:this={heading} tabindex="-1">{t('onboarding.done.title')}</h1>
        <p class="lede">{t('onboarding.done.body')}</p>
        <ul class="summary">
          <li><Icon name="check" size={16} strokeWidth={2.4} />{t('onboarding.done.summary_lang', { value: t(`lang.${i18n.lang}`) })}</li>
          <li><Icon name="check" size={16} strokeWidth={2.4} />{t('onboarding.done.summary_privacy', { value: enabledPrivacy })}</li>
          <li><Icon name="check" size={16} strokeWidth={2.4} />{t('onboarding.done.summary_connect', { value: hooksOn ? t('common.yes') : t('common.no') })}</li>
          <li><Icon name="check" size={16} strokeWidth={2.4} />{t('onboarding.done.summary_plan', { value: tk(`plan.${plan}`) })}</li>
        </ul>
      {/if}

      <footer class="nav">
        {#if index > 0}<Button variant="ghost" icon="chevron_left" onclick={back}>{t('common.back')}</Button>{:else}<span></span>{/if}
        <div class="right">
          {#if step === 'connect' && !hooksOn}<Button variant="ghost" onclick={next}>{t('common.skip')}</Button>{/if}
          {#if step === 'done'}
            <Button variant="primary" size="lg" busy={saving} onclick={next}>{t('onboarding.done.open')}</Button>
          {:else if !(step === 'connect' && !hooksOn)}
            <Button variant="primary" busy={saving} onclick={next}>{t('common.next')}</Button>
          {/if}
        </div>
      </footer>
    </main>
  </div>
</div>

<style>
  .onb {
    min-height: 100vh;
    padding: var(--sp-5) var(--sp-5) var(--sp-8);
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    max-width: 1040px;
    margin: 0 auto var(--sp-8);
  }
  .frame {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: var(--sp-8);
    max-width: 1040px;
    margin: 0 auto;
    align-items: start;
  }
  .rail ol {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    position: sticky;
    top: var(--sp-6);
  }
  .rail li {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: 40px;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .num {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    font-size: var(--fs-xs);
    font-variant-numeric: tabular-nums;
  }
  .rail li.current {
    color: var(--ink);
  }
  .rail li.current .num {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-ink);
  }
  .rail li.done {
    color: var(--ink-2);
  }
  .rail li.done .num {
    border-color: var(--accent);
    color: var(--accent);
  }
  .panel {
    padding: var(--sp-7) var(--sp-8);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--surface);
    min-height: 520px;
    display: flex;
    flex-direction: column;
  }
  .count {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .bar {
    display: none;
    height: 4px;
    margin-top: var(--sp-2);
    border-radius: var(--r-pill);
    background: var(--meter-track);
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }
  h1 {
    margin-top: var(--sp-3);
    font-size: var(--fs-3xl);
    letter-spacing: -0.025em;
    max-width: 22ch;
  }
  h1:focus {
    outline: none;
  }
  .lede {
    margin-top: var(--sp-3);
    color: var(--ink-2);
    font-size: var(--fs-md);
    max-width: 60ch;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: var(--sp-6) 0 0;
    min-width: 0;
  }
  legend {
    font-weight: var(--fw-semibold);
    margin-bottom: var(--sp-3);
    padding: 0;
  }
  .choices {
    display: grid;
    gap: var(--sp-3);
    max-width: 420px;
  }
  .choice {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: var(--sp-4);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    cursor: pointer;
    background: var(--surface);
  }
  .choice:hover {
    border-color: var(--control-border);
  }
  .choice.selected {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
    background: var(--accent-soft);
  }
  .choice input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .choice:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .choice-title {
    font-weight: var(--fw-semibold);
  }
  .choice :global(.tick) {
    margin-left: auto;
    color: var(--accent);
  }
  .plans {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--sp-3);
  }
  .plans legend {
    grid-column: 1 / -1;
  }
  .plan {
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }
  .choice-sub {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    min-height: 1.4em;
  }
  .price {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    margin-top: var(--sp-5);
    max-width: 320px;
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .price input {
    min-height: 40px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--control-border);
    border-radius: var(--r-sm);
    background: var(--surface);
    font-size: var(--fs-base);
  }
  .hint {
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-regular);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-5);
    margin: var(--sp-6) 0;
  }
  dt {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  dd {
    margin-top: 2px;
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
    overflow-wrap: anywhere;
  }
  dd code {
    font-size: var(--fs-base);
    font-weight: var(--fw-medium);
  }
  .import {
    padding: var(--sp-4) var(--sp-5);
    border-radius: var(--r-md);
    background: var(--surface-2);
  }
  .import-title {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-weight: var(--fw-semibold);
  }
  .import-num {
    margin-left: auto;
    color: var(--ink-2);
    font-weight: var(--fw-regular);
    font-variant-numeric: tabular-nums;
  }
  .import :global(.spin) {
    animation: spin 2.4s linear infinite;
    color: var(--accent);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .pbar {
    margin: var(--sp-3) 0 var(--sp-2);
    height: 8px;
    border-radius: var(--r-pill);
    background: var(--meter-track);
  }
  .pbar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--meter-fill);
    transition: width 600ms var(--ease);
  }
  .switches {
    margin-top: var(--sp-5);
  }
  .connect {
    margin-top: var(--sp-6);
  }
  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    margin-top: var(--sp-6);
  }
  .summary li {
    display: flex;
    gap: var(--sp-2);
    align-items: flex-start;
  }
  .summary :global(svg) {
    margin-top: 3px;
    color: var(--good-ink);
  }
  .nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--sp-3);
    margin-top: auto;
    padding-top: var(--sp-7);
  }
  .right {
    display: flex;
    gap: var(--sp-2);
  }
  @media (max-width: 860px) {
    .frame {
      grid-template-columns: minmax(0, 1fr);
      gap: 0;
    }
    .rail {
      display: none;
    }
    .bar {
      display: block;
    }
    .panel {
      padding: var(--sp-6) var(--sp-5);
      min-height: 0;
    }
    .top {
      margin-bottom: var(--sp-5);
    }
    .facts {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
