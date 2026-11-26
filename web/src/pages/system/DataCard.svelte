<script lang="ts">
  import type { SystemInfo } from '../../lib/api/contract';
  import { fmt, t } from '../../lib/i18n/index.svelte';
  import Card from '../../lib/ui/Card.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import { bytes } from './util';

  let { d, refreshing = false }: { d: SystemInfo; refreshing?: boolean } = $props();
  const db = $derived(d.database);
  const p = $derived(d.pricing);
  const builtin = $derived(p.source === 'built-in' || p.source === 'builtin' || p.source === 'embedded');
</script>

<Card id="data" title={t('system.data.title')} subtitle={t('system.data.subtitle')} {refreshing}>
  <div class="cols">
    <div>
      <h3 class="subhead">{t('system.data.encryption')}</h3>
      <p class="lock" class:off={!db.encrypted && !d.demo} class:demo={!db.encrypted && d.demo}>
        <Icon name={db.encrypted ? 'lock' : d.demo ? 'info' : 'warning'} size={18} />
        <span>{db.encrypted ? t('system.data.encrypted') : t('system.data.not_encrypted')}</span>
      </p>
      {#if !db.encrypted}<p class="hint">{t(d.demo ? 'system.data.demo_encryption' : 'system.data.unencrypted_note')}</p>{/if}
      <dl class="facts">
        <div><dt>{t('system.data.key')}</dt><dd>{t(`system.data.key_${db.key_source}`)}</dd></div>
        <div><dt>{t('system.data.size')}</dt><dd class="num">{bytes(db.size_bytes)}</dd></div>
        <div><dt>{t('system.data.schema')}</dt><dd class="num">{db.schema_version}</dd></div>
        <div><dt>{t('system.data.folder')}</dt><dd><code>{d.data_dir}</code></dd></div>
        <div><dt>{t('system.data.claude_folder')}</dt><dd><code>{d.claude_dir}</code></dd></div>
      </dl>
    </div>
    <div>
      <h3 class="subhead">{t('system.data.prices')}</h3>
      <dl class="facts">
        <div><dt>{t('system.data.price_version')}</dt><dd>{p.version}</dd></div>
        <div><dt>{t('system.data.price_source')}</dt><dd>{builtin ? t('system.data.source_builtin') : p.source}</dd></div>
        <div>
          <dt>{t('system.data.signed')}</dt>
          <dd>{builtin && !p.signed ? t('system.data.signed_builtin') : p.signed ? t('system.data.signed_yes') : t('system.data.signed_no')}</dd>
        </div>
        <div><dt>{t('system.data.priced')}</dt><dd class="num">{fmt.number(p.models)}</dd></div>
        <div>
          <dt>{t('system.data.unpriced')}</dt>
          <dd>
            {#if p.unpriced_models.length}
              <ul class="models">
                {#each p.unpriced_models as m (m)}<li><Icon name="warning" size={14} />{m}</li>{/each}
              </ul>
            {:else}
              <span class="none">{t('system.data.unpriced_none')}</span>
            {/if}
          </dd>
        </div>
      </dl>
      {#if p.unpriced_models.length}<p class="note warn"><Icon name="info" size={16} /><span>{t('system.data.unpriced_help')}</span></p>{/if}
    </div>
  </div>
</Card>

<style>
  .cols {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--sp-7);
  }
  .lock {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin-bottom: var(--sp-3);
    color: var(--good-ink);
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
  }
  .lock.off {
    color: var(--warning-ink);
  }
  .lock.demo {
    color: var(--ink-2);
  }
  .facts {
    display: flex;
    flex-direction: column;
  }
  .facts > div {
    display: grid;
    grid-template-columns: minmax(120px, 40%) minmax(0, 1fr);
    gap: var(--sp-3);
    padding: var(--sp-2) 0;
    border-top: 1px solid var(--line);
  }
  dt {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  dd {
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    overflow-wrap: anywhere;
  }
  .models {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .models li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }
  .models :global(svg) {
    color: var(--warning-ink);
  }
  .none {
    color: var(--ink-2);
    font-weight: var(--fw-regular);
  }
  .note {
    margin-top: var(--sp-3);
  }
  @media (max-width: 860px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--sp-5);
    }
  }
</style>
