import { describe, expect, test } from 'vitest';
import { ApiClient } from '../api/client';
import { createMock, type MockOptions } from './index';

const NOW = new Date(2026, 10, 26, 15, 30).getTime();

function client(opts: MockOptions = {}) {
  const mock = createMock({ clock: () => NOW, latencyMs: 0, ...opts });
  return new ApiClient({ baseUrl: 'http://127.0.0.1:7341/api/v1/', fetch: mock.fetch });
}

describe('mock limits, insights, alerts and budgets', () => {
  test('blocks distinguish anchored windows from inferred ones', async () => {
    const d = await client().limits();
    expect(d.blocks.some((b) => b.anchored)).toBe(true);
    expect(d.blocks.every((b) => b.anchored === (b.peak_pct !== null))).toBe(true);
    const all = await client().blocks();
    expect(all.some((b) => !b.anchored)).toBe(true);
    const est = await client({ scenario: 'nostatusline' }).limits();
    expect(est.blocks.every((b) => !b.anchored && b.peak_pct === null)).toBe(true);
    expect(est.usd_per_pct_weekly.every((w) => w.usd_per_pct === null)).toBe(true);
  });

  test('cost per percentage point varies by week and matches the live window', async () => {
    const d = await client().limits();
    const read = d.usd_per_pct_weekly.filter((w) => w.usd_per_pct !== null);
    expect(read.length).toBeGreaterThanOrEqual(4);
    expect(new Set(read.map((w) => w.usd_per_pct!.toFixed(2))).size).toBeGreaterThan(2);
    expect(d.usd_per_pct_weekly.some((w) => w.usd_per_pct === null)).toBe(true);
    expect(read.at(-1)!.usd_per_pct!).toBeCloseTo(d.current.seven_day!.usd_per_pct!, 6);
  });

  test('the gateway scenario reports a spend limit', async () => {
    expect((await client().limits()).current.spend).toBeNull();
    const s = (await client({ scenario: 'gateway' }).overview()).limits.spend!;
    expect(s.limit_usd).toBe(150);
    expect(s.used_pct).toBeCloseTo((s.used_usd! / s.limit_usd!) * 100, 6);
    expect(s.resets_at).toBeGreaterThan(NOW);
  });

  test('insights link to real sessions', async () => {
    const api = client();
    const i = await api.insights();
    expect(i.cache.recent_misses.length).toBeGreaterThan(2);
    expect(i.cache.breakers.length).toBeGreaterThan(0);
    expect(i.cache.recent_misses.some((m) => m.correlated_changes.length > 0)).toBe(true);
    expect(i.model_mix.map((m) => m.code)).toContain('subagents_to_haiku');
    for (const m of i.model_mix) expect(m.saving_usd).toBeCloseTo(m.current_usd - m.projected_usd, 6);
    const ids = [...i.cache.recent_misses.map((m) => m.session_id), ...i.anomalies.flatMap((a) => (a.session_id ? [a.session_id] : []))];
    for (const id of ids) expect((await api.session(id)).summary.id).toBe(id);
    expect(i.anomalies.map((a) => a.kind)).toEqual(expect.arrayContaining(['runaway_session', 'agent_loop']));
  });

  test('alerts carry link targets and budgets honour every scope', async () => {
    const api = client();
    const alerts = await api.alerts({ limit: 50 });
    const ctx = alerts.items.find((a) => a.kind === 'context_near_limit')!;
    const sid = (ctx.params as { session_id: string }).session_id;
    expect((await api.session(sid)).summary.id).toBe(sid);
    expect(alerts.items.some((a) => a.severity === 'critical')).toBe(true);
    const budgets = await api.budgets();
    const client_ = budgets.find((b) => b.scope === 'client')!;
    const global = budgets.find((b) => b.scope === 'global' && b.period === 'month')!;
    expect(client_.scope_label).toBe('Northwind Logistics');
    expect(client_.spent_usd).toBeGreaterThan(0);
    expect(client_.spent_usd).toBeLessThan(global.spent_usd);
    const created = await api.createBudget({ name: 'Team account', scope: 'account', scope_value: 'acct-team', period: 'week', amount_usd: 200, warn_pct: 80, enabled: true });
    expect(created.scope_label).toBe('acct-team');
    await api.deleteBudget(created.id);
    expect((await api.budgets()).some((b) => b.id === created.id)).toBe(false);
    expect(await client({ scenario: 'empty' }).budgets()).toEqual([]);
  });
});
