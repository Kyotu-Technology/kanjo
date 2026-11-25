import { expect, test } from 'vitest';
import type { LimitWindow } from './api/contract';
import { pickForecast } from './forecast';

const w = (over: Partial<LimitWindow>): LimitWindow => ({
  used_pct: 50,
  resets_at: 10_000,
  observed_at: 0,
  stale: false,
  rate_pct_per_hour: 1,
  exhausts_at: null,
  will_exhaust: false,
  usd_in_window: 10,
  usd_per_pct: 0.2,
  daily_budget_pct: 12,
  confidence: 'official',
  ...over,
});

test('the window that runs out first leads', () => {
  const f = pickForecast({ five_hour: w({ will_exhaust: true, exhausts_at: 500 }), seven_day: w({ will_exhaust: true, exhausts_at: 900 }), spend: null, estimate: null });
  expect(f).toMatchObject({ kind: 'exhaust', window: 'five_hour', at: 500 });
});

test('without exhaustion the weekly window is described', () => {
  expect(pickForecast({ five_hour: w({}), seven_day: w({ resets_at: 7 }), spend: null, estimate: null })).toMatchObject({ kind: 'lasts', window: 'seven_day', resetsAt: 7 });
});

test('falls back to the estimate, then to nothing', () => {
  const estimate = { p90_block_usd: 10, blocks_observed: 12, current_block_usd: 4, current_block_pct: 40, confidence: 'estimate' as const };
  expect(pickForecast({ five_hour: null, seven_day: null, spend: null, estimate })).toEqual({ kind: 'estimate', pct: 40, blocks: 12 });
  expect(pickForecast({ five_hour: null, seven_day: null, spend: null, estimate: null })).toEqual({ kind: 'none' });
});
