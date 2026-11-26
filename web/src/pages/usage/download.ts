import type { ExportFormat, ExportView, QueryParams } from '../../lib/api/client';
import { api } from '../../lib/stores/app.svelte';

/** Fetches an export through the API client and hands it to the browser as a download. Returns the file name. */
export async function download(view: ExportView, format: ExportFormat, q: QueryParams): Promise<string> {
  const { blob, filename } = await api().exportView(view, format, q);
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return filename;
}
