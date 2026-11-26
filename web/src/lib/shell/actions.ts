import { asApiError } from '../api/errors';
import { t } from '../i18n/index.svelte';
import { api, describeError } from '../stores/app.svelte';
import { toasts } from '../stores/toast.svelte';
import { buildExport, type PageExport } from './exports.svelte';

/** Downloads the export of a view and reports the result as a toast. */
export async function runExport(e: PageExport): Promise<void> {
  try {
    const { blob, filename } = await buildExport(e, api());
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    toasts.push('success', t('palette.exported', { file: filename }));
  } catch (err) {
    const x = asApiError(err);
    toasts.push('error', x.code === 'unknown' ? t('palette.export_failed') : describeError(x));
  }
}
