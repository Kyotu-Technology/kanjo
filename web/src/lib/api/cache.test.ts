import { describe, expect, test, vi } from 'vitest';
import { QueryCache, queryKey, type Snapshot } from './cache';
import { ApiRequestError } from './errors';

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('QueryCache', () => {
  test('fetches once for concurrent observers and shares data', async () => {
    const cache = new QueryCache();
    const fetch = vi.fn(async () => 42);
    const a: Snapshot<number>[] = [];
    const b: Snapshot<number>[] = [];
    cache.observe({ key: 'x', fetch }, (s) => a.push(s));
    cache.observe({ key: 'x', fetch }, (s) => b.push(s));
    await tick();
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(a.at(-1)?.data).toBe(42);
    expect(b.at(-1)?.status).toBe('success');
  });

  test('serves fresh data without refetching and revalidates stale data in the background', async () => {
    let now = 0;
    const cache = new QueryCache({ now: () => now, staleMs: 1000 });
    let n = 0;
    const fetch = vi.fn(async () => ++n);
    const stop = cache.observe({ key: 'k', fetch }, () => undefined);
    await tick();
    stop();
    now = 500;
    cache.observe({ key: 'k', fetch }, () => undefined);
    expect(fetch).toHaveBeenCalledTimes(1);
    now = 2000;
    const states: Snapshot<number>[] = [];
    cache.observe({ key: 'k', fetch }, (s) => states.push(s));
    expect(states[0].data).toBe(1);
    expect(states[1].fetching).toBe(true);
    expect(states[1].data).toBe(1);
    await tick();
    expect(states.at(-1)?.data).toBe(2);
  });

  test('invalidate by tag refetches observed entries only', async () => {
    const cache = new QueryCache();
    const usage = vi.fn(async () => 'u');
    const other = vi.fn(async () => 'o');
    const idle = vi.fn(async () => 'i');
    cache.observe({ key: 'usage', fetch: usage, tags: ['usage'] }, () => undefined);
    cache.observe({ key: 'other', fetch: other, tags: ['limits'] }, () => undefined);
    const stop = cache.observe({ key: 'idle', fetch: idle, tags: ['usage'] }, () => undefined);
    await tick();
    stop();
    expect(cache.invalidate(['usage'])).toBe(1);
    await tick();
    expect(usage).toHaveBeenCalledTimes(2);
    expect(other).toHaveBeenCalledTimes(1);
    expect(idle).toHaveBeenCalledTimes(1);
    cache.observe({ key: 'idle', fetch: idle, tags: ['usage'] }, () => undefined);
    expect(idle).toHaveBeenCalledTimes(2);
  });

  test('keeps old data when a revalidation fails', async () => {
    const cache = new QueryCache({ staleMs: 0 });
    let fail = false;
    const fetch = async () => {
      if (fail) throw new ApiRequestError(500, 'internal', 'boom');
      return 'ok';
    };
    const seen: Snapshot<string>[] = [];
    cache.observe({ key: 'e', fetch }, (s) => seen.push(s));
    await tick();
    fail = true;
    await cache.refetch('e');
    const last = seen.at(-1)!;
    expect(last.data).toBe('ok');
    expect(last.error?.code).toBe('internal');
    expect(last.status).toBe('error');
  });

  test('update patches data locally', async () => {
    const cache = new QueryCache();
    cache.observe({ key: 'o', fetch: async () => ({ a: 1 }) }, () => undefined);
    await tick();
    expect(cache.update<{ a: number }>('o', (o) => ({ a: o.a + 1 }))).toBe(true);
    expect(cache.peek<{ a: number }>('o')?.data?.a).toBe(2);
  });

  test('unobserved entries are collected', async () => {
    let now = 0;
    const cache = new QueryCache({ now: () => now, gcMs: 100 });
    const stop = cache.observe({ key: 'g', fetch: async () => 1 }, () => undefined);
    await tick();
    stop();
    now = 1000;
    cache.observe({ key: 'h', fetch: async () => 1 }, () => undefined)();
    expect(cache.peek('g')).toBeUndefined();
    expect(cache.peek('h')).toBeDefined();
  });

  test('queryKey is stable and skips empty params', () => {
    expect(queryKey('usage', { to: 2, from: 1, project: undefined, q: '' })).toBe('usage?from=1&to=2');
    expect(queryKey('overview')).toBe('overview');
  });
});
