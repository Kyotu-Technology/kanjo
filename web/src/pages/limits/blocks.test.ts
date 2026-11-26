import { expect, test } from 'vitest';
import type { BreakdownRow } from '../../lib/api/contract';
import { topRows } from './blocks';

const row = (key: string, cost: number, requests = 1): BreakdownRow =>
  ({ key, label: key, usage: { tokens: { input: 0, output: 0, cache_read: 0, cache_write_5m: 0, cache_write_1h: 0 }, cost_usd: cost, requests, unpriced_requests: 0 }, share: 0 }) as BreakdownRow;

test('a block lists its costliest projects first and counts the rest', () => {
  const r = topRows([row('a', 1), row('b', 5), row('c', 3), row('d', 2), row('e', 0, 0)], 3);
  expect(r.top.map((x) => x.key)).toEqual(['b', 'c', 'd']);
  expect(r.more).toBe(1);
});
