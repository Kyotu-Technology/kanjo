import { expect, test } from 'vitest';
import type { ApiClient } from '../api/client';
import { buildExport } from './exports.svelte';

test('a server export carries the page filters', async () => {
  const calls: unknown[] = [];
  const client: Pick<ApiClient, 'exportView'> = {
    exportView: async (view, format, q) => {
      calls.push([view, format, q]);
      return { blob: new Blob(['x']), filename: 'f.csv' };
    },
  };
  await buildExport({ kind: 'server', view: 'sessions', query: { project: 3, from: 1, to: 2, q: 'fix' } }, client);
  expect(calls).toEqual([['sessions', 'csv', { project: 3, from: 1, to: 2, q: 'fix' }]]);
});

test('a view the server cannot export is saved as the JSON the page shows', async () => {
  const client = { exportView: async () => ({ blob: new Blob([]), filename: '' }) };
  const d = await buildExport({ kind: 'json', name: 'limits', data: async () => ({ a: 1 }) }, client, new Date(2026, 10, 26, 12).getTime());
  expect(d.filename).toBe('kanjo-limits-2026-11-26.json');
  expect(JSON.parse(await d.blob.text())).toEqual({ a: 1 });
});
