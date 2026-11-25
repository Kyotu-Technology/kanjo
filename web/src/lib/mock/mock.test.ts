import { describe, expect, test } from 'vitest';
import { ApiClient } from '../api/client';
import { createMock, type MockOptions } from './index';

const NOW = new Date(2026, 10, 26, 15, 30).getTime();
const near = (a: number, b: number) => expect(Math.abs(a - b)).toBeLessThan(1e-6);

function client(opts: MockOptions = {}) {
  const mock = createMock({ clock: () => NOW, latencyMs: 0, ...opts });
  return { mock, api: new ApiClient({ baseUrl: 'http://127.0.0.1:7341/api/v1/', fetch: mock.fetch }) };
}

describe('mock API', () => {
  test('numbers agree across views', async () => {
    const { api } = client();
    const o = await api.overview();
    const dailyTotal = o.daily.reduce((a, b) => a + b.usage.cost_usd, 0);
    near(dailyTotal, o.top_models.reduce((a, r) => a + r.usage.cost_usd, 0));
    near(dailyTotal, o.top_projects.reduce((a, r) => a + r.usage.cost_usd, 0));
    for (const b of o.daily) near(b.by_model.reduce((a, s) => a + s.cost_usd, 0), b.usage.cost_usd);
    near(o.daily.at(-1)!.usage.cost_usd, o.today.usage.cost_usd);
    const usage = await api.usage({ from: o.daily[0].start, to: NOW + 1, group: 'day' });
    near(usage.total.cost_usd, dailyTotal);
    const byProject = await api.breakdown({ by: 'project', from: o.daily[0].start, to: NOW + 1 });
    near(byProject.rows.reduce((a, r) => a + r.share, 0), 1);
    const projects = await api.projects();
    near(projects.reduce((a, p) => a + p.usage_30d.cost_usd, 0), dailyTotal);
    const sessions = await api.sessions({ limit: 100_000 });
    near(sessions.items.reduce((a, s) => a + s.usage.cost_usd, 0), projects.reduce((a, p) => a + p.usage_total.cost_usd, 0));
    expect(o.active_sessions.length).toBeGreaterThan(0);
    expect(o.active_block?.is_active).toBe(true);
    near(o.limits.five_hour!.usd_in_window, o.active_block!.usage.cost_usd);
  });

  test('limits are official with a forecast in the default scenario', async () => {
    const o = await client().api.overview();
    expect(o.limits.seven_day?.confidence).toBe('official');
    expect(o.limits.seven_day?.used_pct).toBeGreaterThan(30);
    expect(o.data.statusline_connected).toBe(true);
    expect(o.data.unpriced_models).toEqual([]);
  });

  test('scenarios produce their states', async () => {
    const empty = await client({ scenario: 'empty' }).api.overview();
    expect(empty.daily.every((d) => d.usage.requests === 0)).toBe(true);
    expect(empty.limits.seven_day).toBeNull();
    const nostatus = await client({ scenario: 'nostatusline' }).api.overview();
    expect(nostatus.limits.seven_day).toBeNull();
    expect(nostatus.limits.estimate?.confidence).toBe('estimate');
    const unpriced = await client({ scenario: 'unpriced' }).api.overview();
    expect(unpriced.data.unpriced_models).toEqual(['claude-nova-preview']);
    expect(unpriced.month.usage.unpriced_requests).toBeGreaterThan(0);
    expect((await client({ scenario: 'import' }).api.system()).ingest.backfill_running).toBe(true);
    const onboarding = await client({ scenario: 'onboarding' }).api.settings();
    expect(onboarding.onboarding_done).toBe(false);
    expect(onboarding.privacy).toEqual({ session_titles: false, redact_paths: false, conversation_viewer: false });
  });

  test('the unauthorized scenario answers 401 with an ApiError body', async () => {
    await expect(client({ scenario: 'unauthorized' }).api.settings()).rejects.toMatchObject({ code: 'unauthorized', status: 401 });
  });

  test('mutations need the CSRF header and change state', async () => {
    const { mock, api } = client({ scenario: 'onboarding' });
    const raw = await mock.fetch('http://127.0.0.1:7341/api/v1/alerts/ack', { method: 'POST', body: '{"id":null}' });
    expect(raw.status).toBe(403);
    const hooks = await api.installHooks({ statusline: true, session_hooks: true, service: false });
    expect(hooks.statusline_installed).toBe(true);
    expect((await api.audit()).items[0].action).toBe('hooks.install');
    const s = await api.settings();
    await api.saveSettings({ ...s, onboarding_done: true });
    expect((await api.settings()).onboarding_done).toBe(true);
  });

  test('transcript is disabled by default and errors are ApiError shaped', async () => {
    const { api } = client();
    const id = (await api.sessions({ limit: 1 })).items[0].id;
    await expect(api.transcript(id)).rejects.toMatchObject({ code: 'disabled', status: 403 });
    await expect(api.session('missing')).rejects.toMatchObject({ code: 'not_found' });
    expect((await api.session(id)).summary.id).toBe(id);
  });

  test('alerts render in the requested language and acknowledge', async () => {
    const { api } = client();
    const pl = await api.alerts({ lang: 'pl', q: 'open' });
    expect(pl.items[0].title.length).toBeGreaterThan(0);
    const { acknowledged } = await api.ackAlert(pl.items[0].id);
    expect(acknowledged).toBe(1);
    expect((await api.alerts({ q: 'open' })).unacknowledged).toBe(pl.unacknowledged - 1);
  });

  test('search finds projects and sessions', async () => {
    const r = await client().api.search('atlas');
    expect(r.some((x) => x.kind === 'project' && x.label === 'atlas-billing')).toBe(true);
    expect(r.some((x) => x.kind === 'session')).toBe(true);
  });

  test('export returns a CSV file', async () => {
    const d = await client().api.exportView('usage', 'csv');
    expect(d.filename).toBe('kanjo-usage-sample.csv');
    expect((await d.blob.text()).split('\n')[0]).toBe('bucket,cost_usd,requests,input,output,cache_read,cache_write');
  });
});
