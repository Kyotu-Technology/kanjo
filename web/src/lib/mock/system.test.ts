import { describe, expect, test } from 'vitest';
import { ApiClient } from '../api/client';
import { createMock, type MockOptions } from './index';

const NOW = new Date(2026, 10, 26, 15, 30).getTime();

function client(opts: MockOptions = {}) {
  const mock = createMock({ clock: () => NOW, latencyMs: 0, ...opts });
  return { mock, api: new ApiClient({ baseUrl: 'http://127.0.0.1:7341/api/v1/', fetch: mock.fetch }) };
}

describe('mock system and settings', () => {
  test('doctor checks come in the requested language with stable ids', async () => {
    const { api } = client();
    const en = await api.system();
    const pl = await api.system({ lang: 'pl' });
    expect(en.checks.map((c) => c.id)).toEqual(pl.checks.map((c) => c.id));
    expect(en.checks).toHaveLength(16);
    expect(pl.checks.find((c) => c.id === 'database')?.title).toBe('Baza danych');
    expect(en.checks.every((c) => c.status === 'ok')).toBe(true);
    expect(en.hooks.statusline_chain).not.toBeNull();
  });

  test('checks follow the state: an unpriced model and quarantine warn, disconnecting warns', async () => {
    const { api } = client({ scenario: 'unpriced' });
    const s = await api.system();
    expect(s.checks.find((c) => c.id === 'pricing')?.status).toBe('warn');
    expect(s.quarantine.reduce((n, q) => n + q.count, 0)).toBe((await api.overview()).data.quarantined_records);
    await api.removeHooks();
    const after = await api.system();
    expect(after.checks.find((c) => c.id === 'statusline')?.status).toBe('warn');
    expect((await api.audit()).items[0].action).toBe('hooks.uninstall');
  });

  test('settings that the server would refuse are rejected and leave the stored settings alone', async () => {
    const { api } = client();
    const s = await api.settings();
    await expect(api.saveSettings({ ...s, currency: { code: 'zl', usd_rate: 4 } })).rejects.toMatchObject({ code: 'invalid', status: 400 });
    await expect(api.saveSettings({ ...s, alerts: { ...s.alerts, limit_thresholds: [120] } })).rejects.toMatchObject({ code: 'invalid' });
    expect((await api.settings()).currency.code).toBe('USD');
    const saved = await api.saveSettings({ ...s, currency: { code: 'PLN', usd_rate: 4.05 }, retention: { ...s.retention, requests_days: 0 } });
    expect(saved.currency.code).toBe('PLN');
  });

  test('the audit log pages and its chain verifies, unless it was tampered with', async () => {
    const { api } = client();
    const first = await api.audit({ limit: 10, offset: 0 });
    const second = await api.audit({ limit: 10, offset: 10 });
    expect(first.total).toBeGreaterThan(20);
    expect(first.items[0].ts).toBeGreaterThanOrEqual(second.items[0].ts);
    expect(new Set([...first.items, ...second.items].map((e) => e.id)).size).toBe(20);
    expect(first.chain_ok).toBe(true);
    await api.createBackup();
    expect((await api.audit({ limit: 1 })).chain_ok).toBe(true);
    expect((await client({ scenario: 'tampered' }).api.audit()).chain_ok).toBe(false);
  });

  test('purge needs exactly one selector and reports what it removed', async () => {
    const { api } = client();
    await expect(api.purge({ project: 1, session: null, before: null, everything: true })).rejects.toMatchObject({ code: 'invalid' });
    const projects = await api.projects();
    const p = projects[0];
    const report = await api.purge({ project: p.id, session: null, before: null, everything: false });
    expect(report.sessions).toBe(p.sessions);
    expect(report.requests).toBe(p.usage_total.requests);
    expect(report.aggregates).toBeGreaterThan(0);
    expect((await api.projects()).find((x) => x.id === p.id)?.sessions ?? 0).toBe(0);
    const all = await api.purge({ project: null, session: null, before: null, everything: true });
    expect(all.alerts).toBeGreaterThan(0);
    expect((await api.sessions({ limit: 10 })).total).toBe(0);
    expect((await api.audit({ limit: 1 })).items[0].action).toBe('data.purge');
  });

  test('a rescan starts reading the files again', async () => {
    const { api } = client();
    const st = await api.rescan();
    expect(st.backfill_running).toBe(true);
    expect(st.files_done).toBe(0);
  });

  test('the expired scenario refuses calls once the session has run out', async () => {
    let now = NOW;
    const { api } = client({ scenario: 'expired', clock: () => now });
    expect((await api.settings()).onboarding_done).toBe(true);
    now += 60_000;
    await expect(api.settings()).rejects.toMatchObject({ code: 'unauthorized', status: 401 });
  });
});
