import type { Limits, LimitWindow } from './api/contract';

export type ForecastWindow = 'five_hour' | 'seven_day';

export type Forecast =
  | { kind: 'exhaust'; window: ForecastWindow; at: number; resetsAt: number; dailyBudgetPct: number | null }
  | { kind: 'lasts'; window: ForecastWindow; resetsAt: number; dailyBudgetPct: number | null }
  | { kind: 'estimate'; pct: number; blocks: number }
  | { kind: 'none' };

/**
 * The one sentence the dashboard leads with: the window that runs out first,
 * otherwise the weekly window (it matters more), otherwise the calibration
 * estimate when there is no official reading.
 */
export function pickForecast(limits: Limits): Forecast {
  const windows: [ForecastWindow, LimitWindow | null][] = [
    ['five_hour', limits.five_hour],
    ['seven_day', limits.seven_day],
  ];
  const exhausting = windows
    .filter((w): w is [ForecastWindow, LimitWindow] => !!w[1] && w[1].will_exhaust && w[1].exhausts_at !== null)
    .sort((a, b) => (a[1].exhausts_at ?? 0) - (b[1].exhausts_at ?? 0));
  if (exhausting.length) {
    const [window, w] = exhausting[0];
    return { kind: 'exhaust', window, at: w.exhausts_at ?? w.resets_at, resetsAt: w.resets_at, dailyBudgetPct: w.daily_budget_pct };
  }
  const steady = limits.seven_day ? (['seven_day', limits.seven_day] as const) : limits.five_hour ? (['five_hour', limits.five_hour] as const) : null;
  if (steady) return { kind: 'lasts', window: steady[0], resetsAt: steady[1].resets_at, dailyBudgetPct: steady[1].daily_budget_pct };
  if (limits.estimate) return { kind: 'estimate', pct: limits.estimate.current_block_pct, blocks: limits.estimate.blocks_observed };
  return { kind: 'none' };
}
