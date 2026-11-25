<script lang="ts">
  import { queries } from '../lib/api/queries';
  import { useQuery } from '../lib/api/query.svelte';
  import { fmt, t } from '../lib/i18n/index.svelte';
  import Card from '../lib/ui/Card.svelte';
  import Icon from '../lib/ui/Icon.svelte';
  import PageHeader from '../lib/ui/PageHeader.svelte';
  import Skeleton from '../lib/ui/Skeleton.svelte';

  const system = useQuery(() => queries.system());
  const d = $derived(system.data);
  const mb = (b: number) => `${fmt.number(b / 1_000_000, 1)} MB`;
</script>

<PageHeader title={t('pages.system.title')} lede={t('pages.system.lede')} />

{#if d}
  <div class="grid">
    <Card title={t('pages.system.checks')}>
      <ul class="checks">
        {#each d.checks as c (c.id)}
          <li class={c.status}>
            <Icon name={c.status === 'ok' ? 'check' : c.status === 'warn' ? 'warning' : 'critical'} size={18} />
            <div>
              <p class="title">{c.title} <span class="sr-only">({t(`pages.system.check.${c.status}`)})</span></p>
              <p class="detail">{c.detail}</p>
              {#if c.fix}<p class="fix">{c.fix}</p>{/if}
            </div>
          </li>
        {/each}
      </ul>
    </Card>
    <Card title={t('pages.system.database')}>
      <dl class="facts">
        <div><dt>{t('pages.system.encrypted')}</dt><dd>{d.database.encrypted ? t('common.yes') : t('common.no')}</dd></div>
        <div><dt>{t('pages.system.size')}</dt><dd>{mb(d.database.size_bytes)}</dd></div>
        <div class="wide"><dt>{t('pages.system.data_dir')}</dt><dd><code>{d.data_dir}</code></dd></div>
      </dl>
    </Card>
    <Card title={t('pages.system.hooks')}>
      <dl class="facts">
        <div><dt>{t('pages.system.statusline')}</dt><dd>{d.hooks.statusline_installed ? t('pages.system.installed') : t('pages.system.not_installed')}</dd></div>
        <div><dt>{t('pages.system.session_hooks')}</dt><dd>{d.hooks.session_hooks_installed ? t('pages.system.installed') : t('pages.system.not_installed')}</dd></div>
        <div><dt>{t('pages.system.service')}</dt><dd>{d.hooks.service_installed ? t('pages.system.installed') : t('pages.system.not_installed')}</dd></div>
      </dl>
    </Card>
  </div>
  <p class="preview">{t('pages.preview')}</p>
{:else}
  <Card><Skeleton lines={6} /></Card>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--sp-5);
    align-items: start;
  }
  .checks li {
    display: flex;
    gap: var(--sp-3);
    padding: var(--sp-3) 0;
  }
  .checks li + li {
    border-top: 1px solid var(--line);
  }
  .ok :global(svg) {
    color: var(--good-ink);
  }
  .warn :global(svg) {
    color: var(--warning-ink);
  }
  .fail :global(svg) {
    color: var(--critical-ink);
  }
  .title {
    font-weight: var(--fw-semibold);
  }
  .detail,
  .fix {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-4);
  }
  .wide {
    grid-column: 1 / -1;
  }
  dt {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  dd {
    font-weight: var(--fw-semibold);
    overflow-wrap: anywhere;
  }
  .preview {
    margin-top: var(--sp-5);
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
</style>
