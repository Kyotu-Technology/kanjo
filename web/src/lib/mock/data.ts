import type { Alert, AuditEntry, BackupInfo, BudgetInput, HookStatus, IngestStatus, Settings, Tokens, Usage } from '../api/contract';

/**
 * Synthetic dataset behind the mock API. Every view is aggregated from the
 * same request records, so totals agree across pages. All names are invented.
 */

export type Scenario = 'default' | 'onboarding' | 'demo' | 'empty' | 'import' | 'nostatusline' | 'unpriced' | 'unauthorized' | 'redacted' | 'viewer' | 'gateway' | 'expired' | 'tampered';
export const SCENARIOS: readonly Scenario[] = ['default', 'onboarding', 'demo', 'empty', 'import', 'nostatusline', 'unpriced', 'unauthorized', 'redacted', 'viewer', 'gateway', 'expired', 'tampered'];

export interface Price {
  input: number;
  output: number;
  cache_read: number;
  cache_write_5m: number;
  cache_write_1h: number;
}

/** USD per million tokens; illustrative list prices for the synthetic models. */
export const PRICES: Record<string, Price> = {
  'claude-opus-4-5': { input: 5, output: 25, cache_read: 0.5, cache_write_5m: 6.25, cache_write_1h: 10 },
  'claude-sonnet-4-5': { input: 3, output: 15, cache_read: 0.3, cache_write_5m: 3.75, cache_write_1h: 6 },
  'claude-haiku-4-5': { input: 1, output: 5, cache_read: 0.1, cache_write_5m: 1.25, cache_write_1h: 2 },
  'claude-fable-5-2': { input: 2, output: 10, cache_read: 0.2, cache_write_5m: 2.5, cache_write_1h: 4 },
};
export const UNPRICED_MODEL = 'claude-nova-preview';

export interface MockProject {
  id: number;
  name: string;
  path: string;
  client: string | null;
}

export interface MockRecord {
  id: string;
  ts: number;
  session: string;
  project: number;
  model: string;
  agent: string | null;
  tokens: Tokens;
  cost: number | null;
  requests: number;
  tools: string[];
  skill: string | null;
  mcp: string | null;
}

export interface MockSession {
  id: string;
  project: number;
  title: string;
  branch: string;
  started: number;
  last: number;
  ctxPct: number;
}

export interface Dataset {
  scenario: Scenario;
  clock: () => number;
  createdAt: number;
  projects: MockProject[];
  sessions: MockSession[];
  records: MockRecord[];
  /** Records the running import has not reached yet (import scenario). */
  pending: MockRecord[];
  settings: Settings;
  ingest: IngestStatus;
  hooks: HookStatus;
  alerts: Alert[];
  budgets: (BudgetInput & { id: number })[];
  audit: AuditEntry[];
  backups: BackupInfo[];
  /** Assumed ceilings in USD that turn spend into limit percentages. */
  fiveHourCeilingUsd: number;
  weeklyCeilingUsd: number;
  weeklyResetsAt: number;
  /** Since when statusline readings exist; blocks after it are anchored to a real reset. */
  statuslineSince: number | null;
  /** Extra-usage spend limit behind a gateway (gateway scenario). */
  spendLimitUsd: number | null;
  nextId: number;
}

export const PROJECTS: MockProject[] = [
  { id: 1, name: 'atlas-billing', path: '/home/dev/code/atlas-billing', client: 'Northwind Logistics' },
  { id: 2, name: 'harbor-web', path: '/home/dev/code/harbor-web', client: 'Brightline Studio' },
  { id: 3, name: 'quill-api', path: '/home/dev/code/quill-api', client: 'Northwind Logistics' },
  { id: 4, name: 'tern-mobile', path: '/home/dev/code/tern-mobile', client: 'Pinecrest Health' },
  { id: 5, name: 'lumen-docs', path: '/home/dev/code/lumen-docs', client: null },
  { id: 6, name: 'orbit-cli', path: '/home/dev/code/orbit-cli', client: null },
];
const PROJECT_WEIGHTS = [0.27, 0.22, 0.18, 0.14, 0.1, 0.09];

const TITLES = [
  'Fix invoice rounding on refunds',
  'Add retries to the webhook sender',
  'Refactor auth middleware',
  'Write migration for order history',
  'Investigate flaky checkout test',
  'Update onboarding copy',
  'Speed up the search index',
  'Address review feedback',
  'Draft API reference pages',
  'Upgrade build tooling',
  'Add CSV export to reports',
  'Tidy up error messages',
  'Plan the notification service',
  'Port settings screen to new layout',
];
const BRANCHES = ['main', 'feature/csv-export', 'fix/rounding', 'chore/deps', 'feature/retry-queue', 'feature/new-settings', 'fix/flaky-test'];
const AGENTS = ['Explore', 'general-purpose', 'code-reviewer'];
const TOOLS = ['Read', 'Edit', 'Bash', 'Grep', 'Write', 'WebSearch', 'Task'];
const SKILLS = ['commit', 'review', null, null, null];
const MCPS = ['issue-tracker', 'docs-search', null, null, null, null];

const HOUR = 3_600_000;
const DAY = 86_400_000;

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function costOf(model: string, t: Tokens): number | null {
  const p = PRICES[model];
  if (!p) return null;
  return (
    (t.input * p.input + t.output * p.output + t.cache_read * p.cache_read + t.cache_write_5m * p.cache_write_5m + t.cache_write_1h * p.cache_write_1h) /
    1_000_000
  );
}

export function emptyUsage(): Usage {
  return { tokens: { input: 0, output: 0, cache_read: 0, cache_write_5m: 0, cache_write_1h: 0 }, cost_usd: 0, requests: 0, unpriced_requests: 0 };
}

export function addRecord(u: Usage, r: MockRecord): void {
  u.tokens.input += r.tokens.input;
  u.tokens.output += r.tokens.output;
  u.tokens.cache_read += r.tokens.cache_read;
  u.tokens.cache_write_5m += r.tokens.cache_write_5m;
  u.tokens.cache_write_1h += r.tokens.cache_write_1h;
  u.requests += r.requests;
  if (r.cost === null) u.unpriced_requests += r.requests;
  else u.cost_usd += r.cost;
}

export function sumUsage(records: Iterable<MockRecord>): Usage {
  const u = emptyUsage();
  for (const r of records) addRecord(u, r);
  return u;
}

export function totalTokens(t: Tokens): number {
  return t.input + t.output + t.cache_read + t.cache_write_5m + t.cache_write_1h;
}

function pick<T>(rand: () => number, items: readonly T[], weights?: readonly number[]): T {
  if (!weights) return items[Math.floor(rand() * items.length)];
  let x = rand() * weights.reduce((a, b) => a + b, 0);
  for (let i = 0; i < items.length; i++) {
    x -= weights[i];
    if (x <= 0) return items[i];
  }
  return items[items.length - 1];
}

function range(rand: () => number, lo: number, hi: number): number {
  return lo + rand() * (hi - lo);
}

export function startOfDay(ts: number): number {
  const d = new Date(ts);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function defaultSettings(): Settings {
  return {
    onboarding_done: true,
    language: 'auto',
    theme: 'system',
    week_start: 0,
    plan: { kind: 'max', monthly_usd: null },
    currency: { code: 'USD', usd_rate: 1 },
    alerts: {
      limit_thresholds: [80, 90, 95],
      forecast: true,
      budgets: true,
      anomalies: true,
      cache: true,
      context: true,
      context_threshold: 85,
      desktop_notifications: true,
      terminal_bell: false,
    },
    privacy: { session_titles: true, redact_paths: false, conversation_viewer: false },
    retention: { requests_days: 400, aggregates_days: 0, limit_samples_days: 400, alerts_days: 180, audit_days: 400, quarantine_days: 90 },
  };
}

interface SessionPlan {
  start: number;
  minutes: number;
  project: number;
  opusShare: number;
  ongoing: boolean;
  title?: string;
  branch?: string;
}

function generate(now: number, rand: () => number, withUnpriced: boolean): { sessions: MockSession[]; records: MockRecord[] } {
  const sessions: MockSession[] = [];
  const records: MockRecord[] = [];
  const today = startOfDay(now);
  const plans: SessionPlan[] = [];
  for (let d = 119; d >= 0; d--) {
    const day = today - d * DAY + HOUR;
    const dayStart = startOfDay(day);
    const dow = (new Date(dayStart).getDay() + 6) % 7;
    const factor = dow === 5 ? 0.3 : dow === 6 ? 0.15 : 1;
    const growth = 0.75 + 0.25 * ((119 - d) / 119);
    const n = Math.round(range(rand, 2, 5.5) * factor * growth);
    for (let i = 0; i < n; i++) {
      const hour = pick(rand, [8, 9, 10, 11, 13, 14, 15, 16, 17, 19, 21], [3, 6, 6, 5, 4, 6, 6, 5, 3, 1.5, 0.8]);
      const start = dayStart + hour * HOUR + Math.floor(rand() * 50) * 60_000;
      plans.push({
        start,
        minutes: Math.round(range(rand, 25, 210)),
        project: pick(rand, PROJECTS, PROJECT_WEIGHTS).id,
        opusShare: rand() < 0.62 ? 0.85 : 0.2,
        ongoing: false,
      });
    }
  }
  plans.push({ start: now - 97 * 60_000, minutes: 97, project: 1, opusShare: 0.9, ongoing: true, title: 'Fix invoice rounding on refunds', branch: 'fix/rounding' });
  plans.push({ start: now - 38 * 60_000, minutes: 38, project: 2, opusShare: 0.3, ongoing: true, title: 'Add CSV export to reports', branch: 'feature/csv-export' });

  let seq = 0;
  for (const plan of plans) {
    if (plan.start >= now) continue;
    const id = `ses-${(++seq).toString(36).padStart(4, '0')}-${Math.floor(rand() * 46656).toString(36).padStart(3, '0')}`;
    const end = Math.min(plan.start + plan.minutes * 60_000, now - 60_000);
    const s: MockSession = {
      id,
      project: plan.project,
      title: plan.title ?? pick(rand, TITLES),
      branch: plan.branch ?? pick(rand, BRANCHES),
      started: plan.start,
      last: plan.start,
      ctxPct: 0,
    };
    let ts = plan.start;
    let step = 0;
    while (ts <= end) {
      const progress = (ts - plan.start) / Math.max(1, end - plan.start);
      const model = rand() < plan.opusShare ? 'claude-opus-4-5' : 'claude-sonnet-4-5';
      const req = Math.round(range(rand, 2, 6));
      const ctx = 30_000 + progress * range(rand, 60_000, 120_000);
      const tokens: Tokens = {
        input: Math.round(req * range(rand, 900, 4500)),
        output: Math.round(req * range(rand, 700, 3200)),
        cache_read: Math.round(req * ctx),
        cache_write_5m: Math.round(req * range(rand, 1500, 9000)),
        cache_write_1h: step === 0 ? Math.round(range(rand, 8000, 20000)) : 0,
      };
      records.push({
        id: `msg-${records.length.toString(36)}`,
        ts,
        session: id,
        project: plan.project,
        model,
        agent: null,
        tokens,
        cost: costOf(model, tokens),
        requests: req,
        tools: [pick(rand, TOOLS), pick(rand, TOOLS)],
        skill: pick(rand, SKILLS),
        mcp: pick(rand, MCPS),
      });
      if (rand() < 0.3) {
        const sub: Tokens = {
          input: Math.round(range(rand, 3000, 12000)),
          output: Math.round(range(rand, 1500, 6000)),
          cache_read: Math.round(range(rand, 40_000, 160_000)),
          cache_write_5m: Math.round(range(rand, 5000, 25_000)),
          cache_write_1h: 0,
        };
        const roll = rand();
        const subModel = roll < 0.5 ? 'claude-haiku-4-5' : roll < 0.7 ? 'claude-fable-5-2' : 'claude-sonnet-4-5';
        records.push({
          id: `msg-${records.length.toString(36)}`,
          ts: ts + 60_000,
          session: id,
          project: plan.project,
          model: subModel,
          agent: pick(rand, AGENTS),
          tokens: sub,
          cost: costOf(subModel, sub),
          requests: Math.round(range(rand, 3, 9)),
          tools: ['Read', 'Grep'],
          skill: null,
          mcp: null,
        });
      }
      if (withUnpriced && plan.project === 4 && ts > now - 12 * DAY && rand() < 0.35) {
        const tk: Tokens = { input: 4000, output: 2500, cache_read: 60_000, cache_write_5m: 5000, cache_write_1h: 0 };
        records.push({
          id: `msg-${records.length.toString(36)}`,
          ts: ts + 30_000,
          session: id,
          project: plan.project,
          model: UNPRICED_MODEL,
          agent: null,
          tokens: tk,
          cost: null,
          requests: 3,
          tools: ['Edit'],
          skill: null,
          mcp: null,
        });
      }
      s.last = Math.min(ts + 60_000, now);
      s.ctxPct = Math.min(97, (ctx / 200_000) * 100);
      step++;
      ts += Math.round(range(rand, 4, 11) * 60_000);
    }
    if (plan.ongoing) {
      s.last = now - Math.round(range(rand, 20, 70)) * 1000;
      s.ctxPct = plan.project === 1 ? 88 : 41;
    }
    sessions.push(s);
  }
  records.sort((a, b) => a.ts - b.ts);
  return { sessions, records };
}

function hashHex(input: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < input.length; i++) {
    const c = input.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 2654435761);
    h2 = Math.imul(h2 ^ c, 1597334677);
  }
  return ((h2 >>> 0).toString(16).padStart(8, '0') + (h1 >>> 0).toString(16).padStart(8, '0')).repeat(4);
}

export function auditEntry(ds: Dataset, action: string, target: string, detail: Record<string, unknown> = {}): AuditEntry {
  const prev = ds.audit[0]?.hash ?? '0'.repeat(64);
  const entry: AuditEntry = {
    id: ds.audit.length + 1,
    ts: ds.clock(),
    actor: 'local',
    action,
    target,
    detail: { ...detail, client: '127.0.0.1' } as AuditEntry['detail'],
    hash: hashHex(prev + action + target),
  };
  ds.audit.unshift(entry);
  return entry;
}

export interface DatasetOptions {
  scenario?: Scenario;
  seed?: number;
  /** Fixed clock for tests; the browser mock uses the real time. */
  clock?: () => number;
}

export function createDataset(opts: DatasetOptions = {}): Dataset {
  const scenario = opts.scenario ?? 'default';
  const clock = opts.clock ?? (() => Date.now());
  const now = clock();
  const rand = mulberry32(opts.seed ?? 20261126);
  const generated = scenario === 'empty' ? { sessions: [], records: [] } : generate(now, rand, scenario === 'unpriced');
  const settings = defaultSettings();
  if (scenario === 'onboarding' || scenario === 'demo') {
    settings.onboarding_done = false;
    settings.plan = { kind: 'unknown', monthly_usd: null };
    settings.privacy = { session_titles: false, redact_paths: false, conversation_viewer: false };
  }
  if (scenario === 'redacted') settings.privacy.redact_paths = true;
  if (scenario === 'viewer') settings.privacy.conversation_viewer = true;

  let records = generated.records;
  let pending: MockRecord[] = [];
  const importing = scenario === 'import' || scenario === 'onboarding';
  if (importing) {
    const cut = now - 21 * DAY;
    pending = records.filter((r) => r.ts < cut);
    records = records.filter((r) => r.ts >= cut);
  }
  const filesTotal = scenario === 'empty' ? 0 : new Set(generated.sessions.map((s) => s.id)).size;
  const ingest: IngestStatus = {
    backfill_running: importing,
    watcher_running: true,
    files_total: filesTotal,
    files_done: importing ? Math.round(filesTotal * 0.34) : filesTotal,
    bytes_total: filesTotal * 412_000,
    bytes_done: (importing ? Math.round(filesTotal * 0.34) : filesTotal) * 412_000,
    requests_stored: records.reduce((n, r) => n + r.requests, 0),
    last_event_at: records.length ? records[records.length - 1].ts : null,
  };
  const connected = scenario !== 'nostatusline' && scenario !== 'onboarding' && scenario !== 'demo' && scenario !== 'empty';
  const hooks: HookStatus = {
    statusline_installed: connected,
    statusline_chain: null,
    session_hooks_installed: connected,
    last_statusline_at: connected ? now - 40_000 : null,
    service_installed: connected,
  };

  const weeklyResetsAt = startOfDay(now + 4 * DAY) + 9 * HOUR;
  const ds: Dataset = {
    scenario,
    clock,
    createdAt: now,
    projects: PROJECTS.map((p) => ({ ...p })),
    sessions: generated.sessions,
    records,
    pending,
    settings,
    ingest,
    hooks,
    alerts: [],
    budgets: scenario === 'empty' ? [] : [
      { id: 1, name: 'Monthly spend', scope: 'global', scope_value: null, period: 'month', amount_usd: 900, warn_pct: 80, enabled: true },
      { id: 2, name: 'Northwind billing work', scope: 'project', scope_value: '1', period: 'month', amount_usd: 165, warn_pct: 80, enabled: true },
      { id: 3, name: 'Daily cap', scope: 'global', scope_value: null, period: 'day', amount_usd: 60, warn_pct: 75, enabled: true },
      { id: 4, name: 'Northwind Logistics weekly', scope: 'client', scope_value: 'Northwind Logistics', period: 'week', amount_usd: 320, warn_pct: 80, enabled: true },
    ],
    audit: [],
    backups: [],
    fiveHourCeilingUsd: 1,
    weeklyCeilingUsd: 1,
    weeklyResetsAt,
    statuslineSince: connected ? now - 34 * DAY : null,
    spendLimitUsd: scenario === 'gateway' ? 150 : null,
    nextId: 1,
  };
  calibrate(ds);
  seedAlerts(ds);
  if (scenario !== 'empty') {
    ds.backups.push({ file: 'kanjo-backup-auto.db', created_at: now - 2 * DAY, size_bytes: 18_400_000 });
    auditEntry(ds, 'settings.update', 'settings', { keys: ['plan'] });
    auditEntry(ds, 'export', 'usage', { format: 'csv' });
  }
  return ds;
}

/** Picks ceilings so the synthetic limit readings land in a realistic range. */
function calibrate(ds: Dataset): void {
  const now = ds.clock();
  const blockStart = new Date(now - 97 * 60_000);
  blockStart.setMinutes(0, 0, 0);
  const blockCost = ds.records.filter((r) => r.ts >= blockStart.getTime()).reduce((a, r) => a + (r.cost ?? 0), 0);
  const lastHour = ds.records.filter((r) => r.ts >= now - HOUR).reduce((a, r) => a + (r.cost ?? 0), 0);
  const remainingH = Math.max(0, (blockStart.getTime() + 5 * HOUR - now) / HOUR);
  ds.fiveHourCeilingUsd = Math.max(blockCost / 0.47, (blockCost + lastHour * remainingH) / 0.86, 25);
  const weekStart = ds.weeklyResetsAt - 7 * DAY;
  const weekCost = ds.records.filter((r) => r.ts >= weekStart).reduce((a, r) => a + (r.cost ?? 0), 0);
  ds.weeklyCeilingUsd = Math.max(weekCost / 0.66, 150);
}

function seedAlerts(ds: Dataset): void {
  if (ds.scenario === 'empty' || ds.scenario === 'onboarding') return;
  const now = ds.clock();
  const push = (a: Omit<Alert, 'id' | 'title' | 'message'>) => ds.alerts.push({ ...a, id: ds.nextId++, title: '', message: '' });
  const limitBase = (window: string, used: number, ts: number, resets: number) => ({ window, used_pct: used, resets_at: resets, observed_at: ts, evaluated_at: ts, tz_offset_min: -new Date(ts).getTimezoneOffset() });
  push({
    ts: now - 26 * 60_000,
    kind: 'limit_forecast',
    severity: 'warning',
    params: { ...limitBase('seven_day', 57, now - 26 * 60_000, now + 3 * DAY), exhausts_at: now + 2 * DAY, rate_pct_per_hour: 0.9 },
    acknowledged_at: null,
  });
  push({
    ts: now - 3 * HOUR,
    kind: 'budget_warning',
    severity: 'warning',
    params: { budget_id: 2, name: 'Northwind billing work', scope: 'project', scope_value: '1', scope_label: 'atlas-billing', period: 'month', amount_usd: 165, spent_usd: 138.6, spent_pct: 84, warn_pct: 80, projected_usd: 190 },
    acknowledged_at: null,
  });
  const ctxSession = ds.sessions.find((s) => s.project === 1 && s.last >= now - 10 * 60_000);
  const quill = [...ds.sessions].reverse().find((s) => s.project === 3 && s.last < now - 2 * DAY);
  const costly = [...ds.sessions].reverse().find((s) => s.project === 2 && s.last < now - DAY);
  push({ ts: now - 5 * HOUR, kind: 'context_near_limit', severity: 'info', params: { session_id: ctxSession?.id ?? null, project_id: 1, project: 'atlas-billing', ctx_used_pct: 88, ctx_size: 200_000, threshold: 85 }, acknowledged_at: null });
  push({ ts: now - 2 * DAY, kind: 'cache_miss_streak', severity: 'info', params: { session_id: quill?.id ?? null, project_id: 3, project: 'quill-api', misses: 4, window_minutes: 30 }, acknowledged_at: now - 2 * DAY + HOUR });
  push({ ts: costly ? costly.last : now - 30 * HOUR, kind: 'runaway_session', severity: 'critical', params: { session_id: costly?.id ?? null, project_id: 2, project: 'harbor-web', cost_usd: 41.2, median_usd: 13.3, factor: 3.1, baseline_sessions: 40 }, acknowledged_at: (costly ? costly.last : now - 30 * HOUR) + 40 * 60_000 });
  push({ ts: now - 4 * DAY, kind: 'cost_spike', severity: 'warning', params: { cost_usd: 96, median_usd: 40, factor: 2.4, baseline_days: 28 }, acknowledged_at: now - 4 * DAY + 2 * HOUR });
  push({ ts: now - 6 * DAY, kind: 'limit_threshold', severity: 'warning', params: { ...limitBase('five_hour', 91, now - 6 * DAY, now - 6 * DAY + 2 * HOUR), threshold: 90 }, acknowledged_at: now - 6 * DAY + 600_000 });
  ds.alerts.sort((a, b) => b.ts - a.ts);
}

/** Moves part of the pending import into the stored records (import scenario). */
export function advanceImport(ds: Dataset, fraction = 0.12): boolean {
  if (!ds.ingest.backfill_running) return false;
  const take = Math.max(1, Math.ceil(ds.pending.length * fraction));
  const moved = ds.pending.splice(ds.pending.length - take, take);
  ds.records = [...moved, ...ds.records].sort((a, b) => a.ts - b.ts);
  const doneFiles = Math.min(ds.ingest.files_total, ds.ingest.files_done + Math.ceil(ds.ingest.files_total * 0.08));
  ds.ingest = {
    ...ds.ingest,
    files_done: ds.pending.length === 0 ? ds.ingest.files_total : doneFiles,
    bytes_done: (ds.pending.length === 0 ? ds.ingest.files_total : doneFiles) * 412_000,
    requests_stored: ds.records.reduce((n, r) => n + r.requests, 0),
    backfill_running: ds.pending.length > 0,
    last_event_at: ds.clock(),
  };
  return true;
}

/** Appends one fresh request to a running session, as the live feed would. */
export function appendLiveRecord(ds: Dataset, rand: () => number = Math.random): MockRecord | undefined {
  const now = ds.clock();
  const active = ds.sessions.filter((s) => s.last >= now - 15 * 60_000);
  if (!active.length) return undefined;
  const s = active[Math.floor(rand() * active.length)];
  const model = s.project === 1 ? 'claude-opus-4-5' : 'claude-sonnet-4-5';
  const req = 2 + Math.floor(rand() * 3);
  const tokens: Tokens = {
    input: req * 2200,
    output: req * 1800,
    cache_read: req * Math.round(110_000 + rand() * 40_000),
    cache_write_5m: req * 4200,
    cache_write_1h: 0,
  };
  const r: MockRecord = {
    id: `msg-live-${ds.nextId++}`,
    ts: now,
    session: s.id,
    project: s.project,
    model,
    agent: null,
    tokens,
    cost: costOf(model, tokens),
    requests: req,
    tools: ['Edit', 'Bash'],
    skill: null,
    mcp: null,
  };
  ds.records.push(r);
  s.last = now;
  s.ctxPct = Math.min(97, s.ctxPct + 0.6);
  ds.ingest = { ...ds.ingest, requests_stored: ds.ingest.requests_stored + req, last_event_at: now };
  return r;
}
