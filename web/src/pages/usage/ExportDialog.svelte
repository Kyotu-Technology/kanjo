<script lang="ts">
  import type { ExportFormat } from '../../lib/api/client';
  import { asApiError } from '../../lib/api/errors';
  import { fmt, t, tk } from '../../lib/i18n/index.svelte';
  import { describeError } from '../../lib/stores/app.svelte';
  import { toasts } from '../../lib/stores/toast.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Dialog from '../../lib/ui/Dialog.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import { download } from './download';
  import { apiFilters, type UsageFilters } from './state';

  interface Props {
    open: boolean;
    filters: UsageFilters;
    /** Active filters in words, or empty. */
    summary: string;
  }
  let { open = $bindable(), filters, summary }: Props = $props();
  let what = $state<'usage' | 'breakdown'>('usage');
  let format = $state<ExportFormat>('csv');
  let busy = $state(false);

  async function run() {
    busy = true;
    try {
      const base = { from: filters.range.from, to: filters.range.to, ...apiFilters(filters) };
      const file = await download(what, format, what === 'usage' ? { ...base, group: filters.group } : { ...base, by: filters.by });
      toasts.push('success', t('pages.usage.export.done', { file }));
      open = false;
    } catch (e) {
      toasts.push('error', t('pages.usage.export.failed', { message: describeError(asApiError(e)) }));
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={t('pages.usage.export.title')} description={t('pages.usage.export.description')}>
  <form
    id="usage-export"
    onsubmit={(e) => {
      e.preventDefault();
      void run();
    }}
  >
    <fieldset>
      <legend>{t('pages.usage.export.what')}</legend>
      <label class="opt">
        <input type="radio" name="what" value="usage" bind:group={what} />
        <span>{t('pages.usage.export.over_time', { group: tk(`pages.usage.group.${filters.group}`).toLowerCase() })}</span>
      </label>
      <label class="opt">
        <input type="radio" name="what" value="breakdown" bind:group={what} />
        <span>{t('pages.usage.export.breakdown', { dim: filters.by === 'mcp' ? tk(`pages.usage.dims.${filters.by}`) : tk(`pages.usage.dims.${filters.by}`).toLowerCase() })}</span>
      </label>
    </fieldset>
    <fieldset>
      <legend>{t('pages.usage.export.format')}</legend>
      <label class="opt">
        <input type="radio" name="format" value="csv" bind:group={format} />
        <span>{t('pages.usage.export.csv')}</span>
      </label>
      <label class="opt">
        <input type="radio" name="format" value="json" bind:group={format} />
        <span>{t('pages.usage.export.json')}</span>
      </label>
    </fieldset>
    <div class="scope">
      <p>{t('pages.usage.export.scope', { from: fmt.date(filters.range.from, 'long'), to: fmt.date(filters.range.to - 1, 'long') })}</p>
      <p>{summary ? t('pages.usage.export.filters', { filters: summary }) : t('pages.usage.export.no_filters')}</p>
      <p class="audit"><Icon name="lock" size={14} />{t('pages.usage.export.audit')}</p>
    </div>
  </form>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (open = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" icon="download" type="submit" form="usage-export" {busy}>{t('pages.usage.export.download')}</Button>
  {/snippet}
</Dialog>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  legend {
    margin-bottom: var(--sp-2);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: 40px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .opt:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .opt input {
    accent-color: var(--accent);
    margin: 0;
  }
  .scope {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }
  .audit {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin-top: var(--sp-1);
    color: var(--ink-3);
  }
  .audit :global(svg) {
    flex: none;
  }
</style>
