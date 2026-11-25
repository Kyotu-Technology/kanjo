import type { ExportView } from '../api/client';
import { asApiError } from '../api/errors';
import { t } from '../i18n/index.svelte';
import { api, describeError } from '../stores/app.svelte';
import { toasts } from '../stores/toast.svelte';

/** Downloads an export of a view as CSV and reports the result as a toast. */
export async function exportView(view: ExportView): Promise<void> {
  try {
    const { blob, filename } = await api().exportView(view, 'csv');
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    toasts.push('success', t('palette.exported', { file: filename }));
  } catch (e) {
    const err = asApiError(e);
    toasts.push('error', err.code === 'unknown' ? t('palette.export_failed') : describeError(err));
  }
}
