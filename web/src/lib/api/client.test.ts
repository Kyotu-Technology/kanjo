import { describe, expect, test, vi } from 'vitest';
import { ApiClient, buildUrl, defaultBaseUrl } from './client';
import { ApiRequestError } from './errors';

function fakeFetch(status: number, body: unknown, headers: Record<string, string> = {}) {
  return vi.fn(async (_url: string, _init: RequestInit) => new Response(body === undefined ? null : JSON.stringify(body), { status, headers }));
}

describe('ApiClient', () => {
  test('resolves the API root relative to the page, so any base path works', () => {
    expect(defaultBaseUrl({ baseURI: 'https://host:8080/team/kanjo/' })).toBe('https://host:8080/team/kanjo/api/v1/');
    expect(defaultBaseUrl({ baseURI: 'http://127.0.0.1:7341/#/usage' })).toBe('http://127.0.0.1:7341/api/v1/');
  });

  test('builds query strings without empty values', () => {
    expect(buildUrl('http://h/api/v1/', 'usage', { from: 1, to: undefined, project: null, q: '' })).toBe('http://h/api/v1/usage?from=1');
  });

  test('sends the CSRF header on mutations only', async () => {
    const f = fakeFetch(200, { acknowledged: 2 });
    const api = new ApiClient({ baseUrl: 'http://h/api/v1/', fetch: f });
    await api.ackAlert(null);
    const init = f.mock.calls[0][1];
    expect((init.headers as Record<string, string>)['X-Kanjo-CSRF']).toBe('1');
    expect(init.method).toBe('POST');
    expect(init.body).toBe('{"id":null}');
    const g = fakeFetch(200, {});
    await new ApiClient({ baseUrl: 'http://h/api/v1/', fetch: g }).overview();
    expect((g.mock.calls[0][1].headers as Record<string, string>)['X-Kanjo-CSRF']).toBeUndefined();
  });

  test('turns ApiError bodies into typed errors and reports 401', async () => {
    const onUnauthorized = vi.fn();
    const api = new ApiClient({ baseUrl: 'http://h/api/v1/', fetch: fakeFetch(401, { code: 'unauthorized', message: 'token missing' }), onUnauthorized });
    const err = await api.overview().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiRequestError);
    expect((err as ApiRequestError).code).toBe('unauthorized');
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });

  test('maps bodies without ApiError shape by status, and network failures', async () => {
    const e1 = await new ApiClient({ baseUrl: 'http://h/api/v1/', fetch: fakeFetch(404, undefined) }).overview().catch((e: unknown) => e);
    expect((e1 as ApiRequestError).code).toBe('not_found');
    const broken = vi.fn(async () => {
      throw new TypeError('Failed to fetch');
    });
    const e2 = await new ApiClient({ baseUrl: 'http://h/api/v1/', fetch: broken }).overview().catch((e: unknown) => e);
    expect((e2 as ApiRequestError).code).toBe('network');
  });

  test('handles 204 and exports with a filename', async () => {
    const api = new ApiClient({ baseUrl: 'http://h/api/v1/', fetch: fakeFetch(204, undefined) });
    expect(await api.deleteBudget(3)).toBeUndefined();
    const f = vi.fn(async () => new Response('a,b', { status: 200, headers: { 'Content-Disposition': 'attachment; filename="kanjo-usage.csv"' } }));
    const d = await new ApiClient({ baseUrl: 'http://h/api/v1/', fetch: f }).exportView('usage', 'csv');
    expect(d.filename).toBe('kanjo-usage.csv');
    expect(await d.blob.text()).toBe('a,b');
  });
});
