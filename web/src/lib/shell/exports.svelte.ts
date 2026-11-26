import type { ApiClient, Download, ExportFormat, ExportView, QueryParams } from '../api/client';

/**
 * What "export this view" downloads on the page that is open: a server export with
 * the page's own filters, or, for views the server has no export for, the read model
 * the page shows, as JSON.
 */
export type PageExport =
  | { kind: 'server'; view: ExportView; query?: QueryParams; format?: ExportFormat }
  | { kind: 'json'; name: string; data: () => Promise<unknown> };

class ExportState {
  current = $state.raw<PageExport | null>(null);
}

export const pageExport = new ExportState();

/** Registers the export of the mounted page; the options function is reactive, so filters stay current. */
export function usePageExport(fn: () => PageExport | null): void {
  $effect(() => {
    const e = fn();
    pageExport.current = e;
    return () => {
      if (pageExport.current === e) pageExport.current = null;
    };
  });
}

function day(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** Builds the file for an export without touching the page. */
export async function buildExport(e: PageExport, client: Pick<ApiClient, 'exportView'>, now = Date.now()): Promise<Download> {
  if (e.kind === 'server') return client.exportView(e.view, e.format ?? 'csv', e.query ?? {});
  const body = JSON.stringify(await e.data(), null, 2);
  return { blob: new Blob([body], { type: 'application/json' }), filename: `kanjo-${e.name}-${day(now)}.json` };
}
