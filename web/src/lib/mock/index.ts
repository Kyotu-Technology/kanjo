import type { BudgetInput, HookInstallRequest, LiveEvent, ProjectPatch, PurgeRequest, Query, Settings, Usage } from '../api/contract';
import type { FetchLike } from '../api/client';
import type { EventSourceLike } from '../api/live';
import { advanceImport, appendLiveRecord, auditEntry, createDataset, SCENARIOS, type Dataset, type DatasetOptions, type Scenario } from './data';
import * as v from './views';
import { purgeDataset, seedSystem, startRescan, subscribe, validateSettings } from './system';

/**
 * In-browser implementation of every /api/v1 endpoint over a synthetic
 * dataset. Loaded only through a dynamic import, so it stays out of the
 * main bundle.
 */

export { SCENARIOS, type Scenario };

export interface MockOptions extends DatasetOptions {
  /** Simulated network latency in ms (0 in tests). */
  latencyMs?: number;
  /** Interval between synthetic live events. */
  eventIntervalMs?: number;
}

export interface MockServer {
  dataset: Dataset;
  fetch: FetchLike;
  eventSource: (url: string) => EventSourceLike;
}

class HttpError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json', ...headers } });
}

function parseQuery(params: URLSearchParams): Partial<Query> {
  const q: Record<string, unknown> = {};
  for (const [k, val] of params) {
    q[k] = ['from', 'to', 'project', 'limit', 'offset'].includes(k) ? Number(val) : val;
  }
  return q as Partial<Query>;
}

function csv(rows: (string | number | null)[][]): string {
  return rows
    .map((r) =>
      r
        .map((c) => {
          const s = c === null ? '' : String(c);
          return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
        })
        .join(','),
    )
    .join('\n');
}

type Row = (string | number | null)[];

const USAGE_HEADER = ['requests', 'unpriced_requests', 'input_tokens', 'output_tokens', 'cache_read_tokens', 'cache_write_5m_tokens', 'cache_write_1h_tokens', 'total_tokens', 'cost_usd'];

function usageCells(u: Usage): Row {
  const t = u.tokens;
  return [u.requests, u.unpriced_requests, t.input, t.output, t.cache_read, t.cache_write_5m, t.cache_write_1h, t.input + t.output + t.cache_read + t.cache_write_5m + t.cache_write_1h, u.cost_usd.toFixed(6)];
}

function exportBody(ds: Dataset, view: string, q: Partial<Query>): { rows: Row[]; data: unknown } {
  const iso = (ts: number) => new Date(ts).toISOString();
  switch (view) {
    case 'usage': {
      const r = v.usageReport(ds, q);
      const body: Row[] = r.buckets.map((b) => [iso(b.start), b.label, ...usageCells(b.usage), b.by_model.map((m) => m.model).join(';')]);
      return { data: r, rows: [['start', 'label', ...USAGE_HEADER, 'models'], ...body] };
    }
    case 'breakdown': {
      const r = v.breakdown(ds, q);
      const body = r.rows.map((x): Row => [r.by, x.key, x.label, ...usageCells(x.usage), x.share.toFixed(6)]);
      return { data: r, rows: [['dimension', 'key', 'label', ...USAGE_HEADER, 'share'], ...body] };
    }
    case 'sessions': {
      const r = v.sessionList(ds, { ...q, limit: 10_000 });
      const body = r.items.map((s): Row => [
        s.id,
        s.project_id,
        s.project,
        s.title,
        s.git_branch,
        iso(s.started_at),
        iso(s.last_at),
        ...usageCells(s.usage),
        s.main_model,
        s.models.join(';'),
        s.subagent_cost_usd.toFixed(6),
        String(s.is_active),
        s.context_pct === null ? null : s.context_pct.toFixed(1),
        s.cache_hit_ratio === null ? null : s.cache_hit_ratio.toFixed(4),
      ]);
      const head: Row = ['id', 'project_id', 'project', 'title', 'git_branch', 'started_at', 'last_at', ...USAGE_HEADER];
      return { data: r, rows: [[...head, 'main_model', 'models', 'subagent_cost_usd', 'is_active', 'context_pct', 'cache_hit_ratio'], ...body] };
    }
    case 'blocks': {
      const r = v.computeBlocks(ds);
      const body = r.map((b): Row => [iso(b.start), iso(b.end), b.usage.cost_usd.toFixed(4), b.peak_pct === null ? null : b.peak_pct.toFixed(1)]);
      return { data: r, rows: [['start', 'end', 'cost_usd', 'peak_pct'], ...body] };
    }
    case 'turns': {
      const r = v.turns(ds, q);
      const tk = ['ts', 'message_id', 'session_id', 'project', 'model', 'agent_type', ...USAGE_HEADER.slice(2, 8), 'cost_usd', 'stop_reason', 'tools'];
      const body = r.map((x): Row => {
        const t = x.tokens;
        const total = t.input + t.output + t.cache_read + t.cache_write_5m + t.cache_write_1h;
        return [iso(x.ts), x.message_id, x.session_id, x.project, x.model, x.agent_type, t.input, t.output, t.cache_read, t.cache_write_5m, t.cache_write_1h, total, x.cost_usd === null ? null : x.cost_usd.toFixed(6), x.stop_reason, x.tools.join(';')];
      });
      return { data: r, rows: [tk, ...body] };
    }
    case 'alerts': {
      const r = v.alertList(ds, { ...q, limit: 10_000 });
      return { data: r, rows: [['id', 'ts', 'kind', 'severity', 'title'], ...r.items.map((a): Row => [a.id, iso(a.ts), a.kind, a.severity, a.title])] };
    }
    case 'audit': {
      const r = v.auditPage(ds, { limit: 10_000 });
      return { data: r, rows: [['id', 'ts', 'action', 'target'], ...r.items.map((a): Row => [a.id, iso(a.ts), a.action, a.target])] };
    }
    default:
      throw new HttpError(404, 'not_found', `Unknown export view ${view}`);
  }
}

async function readBody<T>(init: RequestInit): Promise<T> {
  if (typeof init.body !== 'string') throw new HttpError(400, 'invalid', 'Missing JSON body');
  try {
    return JSON.parse(init.body) as T;
  } catch {
    throw new HttpError(400, 'invalid', 'Body is not valid JSON');
  }
}

function budgetFromInput(input: BudgetInput): BudgetInput {
  if (!input.name || !(input.amount_usd > 0)) throw new HttpError(400, 'invalid', 'A budget needs a name and a positive amount');
  return input;
}

const ID_PARENTS = ['sessions', 'projects', 'budgets', 'export'];

export async function handle(ds: Dataset, method: string, path: string, params: URLSearchParams, init: RequestInit): Promise<Response> {
  const q = parseQuery(params);
  const seg = path.split('/').filter(Boolean);
  if (method !== 'GET') {
    const headers = new Headers(init.headers);
    if (headers.get('X-Kanjo-CSRF') !== '1') throw new HttpError(403, 'forbidden', 'Missing X-Kanjo-CSRF header');
  }
  const route = `${method} /${seg.map((s, i) => (i === 1 && ID_PARENTS.includes(seg[0]) ? ':id' : s)).join('/')}`;
  const id = decodeURIComponent(seg[1] ?? '');
  switch (route) {
    case 'GET /health':
      return json({ status: 'ok', version: '0.1.0-sample' });
    case 'GET /overview':
      return json(v.overview(ds));
    case 'GET /usage':
      return json(v.usageReport(ds, q));
    case 'GET /breakdown':
      return json(v.breakdown(ds, q));
    case 'GET /sessions':
      return json(v.sessionList(ds, q));
    case 'GET /sessions/:id': {
      const d = v.sessionDetail(ds, id);
      if (!d) throw new HttpError(404, 'not_found', 'No such session');
      return json(d);
    }
    case 'GET /sessions/:id/transcript':
      if (!ds.settings.privacy.conversation_viewer) throw new HttpError(403, 'disabled', 'The transcript viewer is switched off');
      auditEntry(ds, 'transcript.view', id);
      return json(v.transcript(ds, id));
    case 'GET /turns':
      return json(v.turns(ds, q));
    case 'GET /projects':
      return json(v.projects(ds));
    case 'PATCH /projects/:id': {
      const p = ds.projects.find((x) => x.id === Number(id));
      if (!p) throw new HttpError(404, 'not_found', 'No such project');
      const patch = await readBody<ProjectPatch>(init);
      if (patch.name) p.name = patch.name;
      if (patch.client !== undefined && patch.client !== null) p.client = patch.client === '' ? null : patch.client;
      return json(v.projects(ds).find((x) => x.id === p.id));
    }
    case 'GET /blocks':
      return json(v.computeBlocks(ds).filter((b) => (q.from == null || b.end > q.from) && (q.to == null || b.start < q.to)).reverse());
    case 'GET /limits':
      return json(v.limitsDetail(ds));
    case 'GET /heatmap':
      return json(v.heatmap(ds, q));
    case 'GET /compare':
      return json(v.compare(ds, q));
    case 'GET /plan':
      return json(v.planFit(ds));
    case 'GET /insights':
      return json(v.insights(ds));
    case 'GET /alerts':
      return json(v.alertList(ds, q));
    case 'POST /alerts/ack': {
      const body = await readBody<{ id: number | null }>(init);
      let n = 0;
      for (const a of ds.alerts) {
        if (a.acknowledged_at === null && (body.id === null || a.id === body.id)) {
          a.acknowledged_at = ds.clock();
          n++;
        }
      }
      return json({ acknowledged: n });
    }
    case 'GET /budgets':
      return json(v.budgets(ds));
    case 'POST /budgets': {
      const b = { ...budgetFromInput(await readBody<BudgetInput>(init)), id: ds.nextId++ };
      ds.budgets.push(b);
      return json(v.budgets(ds).find((x) => x.id === b.id));
    }
    case 'PUT /budgets/:id': {
      const i = ds.budgets.findIndex((b) => b.id === Number(id));
      if (i < 0) throw new HttpError(404, 'not_found', 'No such budget');
      ds.budgets[i] = { ...budgetFromInput(await readBody<BudgetInput>(init)), id: Number(id) };
      return json(v.budgets(ds)[i]);
    }
    case 'DELETE /budgets/:id':
      ds.budgets = ds.budgets.filter((b) => b.id !== Number(id));
      return json(null, 204);
    case 'GET /settings':
      return json(ds.settings);
    case 'PUT /settings': {
      const next = await readBody<Settings>(init);
      const problem = validateSettings(next);
      if (problem) throw new HttpError(400, 'invalid', problem);
      ds.settings = next;
      auditEntry(ds, 'settings.update', 'settings');
      return json(ds.settings);
    }
    case 'GET /system':
      return json(v.system(ds, typeof q.lang === 'string' ? q.lang : undefined));
    case 'POST /system/hooks': {
      const r = await readBody<HookInstallRequest>(init);
      ds.hooks = {
        ...ds.hooks,
        statusline_installed: ds.hooks.statusline_installed || r.statusline,
        session_hooks_installed: ds.hooks.session_hooks_installed || r.session_hooks,
        service_installed: ds.hooks.service_installed || r.service,
        last_statusline_at: r.statusline ? ds.clock() : ds.hooks.last_statusline_at,
      };
      if (r.statusline && ds.statuslineSince === null) ds.statuslineSince = ds.clock();
      ds.backups.unshift({ file: 'claude-settings-before-hooks.json', created_at: ds.clock(), size_bytes: 2_310 });
      auditEntry(ds, 'hooks.install', 'settings.json', { ...r });
      return json(ds.hooks);
    }
    case 'DELETE /system/hooks':
      ds.hooks = { ...ds.hooks, statusline_installed: false, session_hooks_installed: false };
      auditEntry(ds, 'hooks.remove', 'settings.json');
      return json(ds.hooks);
    case 'GET /system/backups':
      return json(ds.backups);
    case 'POST /system/backup': {
      const b = { file: `kanjo-backup-${new Date(ds.clock()).toISOString().slice(0, 10)}.db`, created_at: ds.clock(), size_bytes: 18_900_000 };
      ds.backups.unshift(b);
      auditEntry(ds, 'backup.create', b.file);
      return json(b);
    }
    case 'POST /system/purge': {
      const req = await readBody<PurgeRequest>(init);
      const report = purgeDataset(ds, req);
      if (typeof report === 'string') throw new HttpError(400, 'invalid', report);
      const target = req.everything ? 'everything' : req.project != null ? `project ${req.project}` : req.session != null ? `session ${req.session}` : `before ${new Date(req.before ?? 0).toISOString().slice(0, 10)}`;
      auditEntry(ds, 'data.purge', target, { ...report });
      return json(report);
    }
    case 'POST /system/rescan':
      return json(startRescan(ds));
    case 'GET /audit':
      return json(v.auditPage(ds, q));
    case 'GET /search':
      return json(v.search(ds, String(q.q ?? '')));
    case 'GET /export/:id': {
      const format = q.format === 'json' ? 'json' : 'csv';
      const { rows, data } = exportBody(ds, id, q);
      auditEntry(ds, 'export', id, { format });
      const file = `kanjo-${id}-sample.${format}`;
      return new Response(format === 'json' ? JSON.stringify(data, null, 2) : csv(rows), {
        status: 200,
        headers: { 'Content-Type': format === 'json' ? 'application/json' : 'text/csv', 'Content-Disposition': `attachment; filename="${file}"` },
      });
    }
    default:
      throw new HttpError(404, 'not_found', `No route for ${method} ${path}`);
  }
}

/** In the expired scenario the session cookie stops working a few seconds after load. */
const EXPIRES_AFTER_MS = 6000;
const expired = (ds: Dataset) => ds.scenario === 'expired' && ds.clock() - ds.createdAt > EXPIRES_AFTER_MS;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

class MockEventSource implements EventSourceLike {
  onopen: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent<string>) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  private timeouts: ReturnType<typeof setTimeout>[] = [];
  private intervals: ReturnType<typeof setInterval>[] = [];
  private closed = false;
  private unsubscribe: () => void = () => undefined;

  constructor(
    private readonly ds: Dataset,
    intervalMs: number,
  ) {
    if (ds.scenario === 'expired') {
      this.timeouts.push(setTimeout(() => this.onerror?.(new Event('error')), expired(ds) ? 120 : Math.max(150, ds.createdAt + EXPIRES_AFTER_MS - ds.clock())));
      if (expired(ds)) return;
    }
    this.timeouts.push(setTimeout(() => this.onopen?.(new Event('open')), 150));
    this.unsubscribe = subscribe(ds, (e) => this.emit(e));
    this.intervals.push(setInterval(() => this.tick(), intervalMs));
    if (ds.ingest.backfill_running) this.intervals.push(setInterval(() => this.importTick(), 1600));
  }

  private emit(e: LiveEvent): void {
    if (this.closed) return;
    this.onmessage?.(new MessageEvent('message', { data: JSON.stringify(e) }));
  }

  private tick(): void {
    const r = appendLiveRecord(this.ds);
    if (!r) return;
    if (this.ds.hooks.statusline_installed) this.ds.hooks = { ...this.ds.hooks, last_statusline_at: this.ds.clock() };
    this.emit({ type: 'usage', data: { requests: r.requests, cost_usd: r.cost ?? 0, sessions: [r.session] } });
    this.emit({ type: 'limits', data: v.limits(this.ds) });
    this.emit({ type: 'status', data: { session_id: r.session } });
  }

  private importTick(): void {
    if (!advanceImport(this.ds)) return;
    this.emit({ type: 'ingest', data: this.ds.ingest });
  }

  close(): void {
    this.closed = true;
    this.unsubscribe();
    for (const t of this.timeouts) clearTimeout(t);
    for (const t of this.intervals) clearInterval(t);
  }
}

export function scenarioFrom(value: string | null | undefined): Scenario {
  return (SCENARIOS as readonly string[]).includes(value ?? '') ? (value as Scenario) : 'default';
}

export function createMock(opts: MockOptions = {}): MockServer {
  const ds = createDataset(opts);
  seedSystem(ds);
  const latency = opts.latencyMs ?? 140;
  const fetch: FetchLike = async (url, init) => {
    const u = new URL(url);
    const at = u.pathname.indexOf('/api/v1/');
    const path = at >= 0 ? u.pathname.slice(at + '/api/v1'.length) : u.pathname;
    if (latency > 0) await sleep(latency * (0.6 + Math.random() * 0.8));
    if (ds.scenario === 'unauthorized' && path !== '/health') return json({ code: 'unauthorized', message: 'Missing or wrong access token' }, 401);
    if (expired(ds) && path !== '/health') return json({ code: 'unauthorized', message: 'The session cookie has expired' }, 401);
    try {
      return await handle(ds, (init.method ?? 'GET').toUpperCase(), path, u.searchParams, init);
    } catch (e) {
      if (e instanceof HttpError) return json({ code: e.code, message: e.message }, e.status);
      return json({ code: 'internal', message: e instanceof Error ? e.message : String(e) }, 500);
    }
  };
  return { dataset: ds, fetch, eventSource: () => new MockEventSource(ds, opts.eventIntervalMs ?? 12_000) };
}
