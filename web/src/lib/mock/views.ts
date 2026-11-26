import type {
  Alert,
  AlertList,
  Anomaly,
  AuditPage,
  Block,
  Breakdown,
  BreakdownRow,
  BurnRate,
  Budget,
  CacheBreaker,
  CacheInsights,
  CacheMiss,
  ConfigChange,
  CacheStatus,
  Comparison,
  Confidence,
  DataStatus,
  Dimension,
  Driver,
  Granularity,
  Heatmap,
  Insights,
  LimitPoint,
  Limits,
  LimitsDetail,
  ModelMixSuggestion,
  ModelSlice,
  Overview,
  PeriodSummary,
  PlanFit,
  Project,
  Query,
  SearchResult,
  SessionDetail,
  SessionEvent,
  SessionList,
  SessionSummary,
  SpendLimit,
  SystemInfo,
  Transcript,
  Turn,
  Usage,
  UsageBucket,
  UsageReport,
  UsdPerPctPoint,
} from '../api/contract';
import { addRecord, costOf, emptyUsage, startOfDay, sumUsage, totalTokens, UNPRICED_MODEL, type Dataset, type MockRecord, type MockSession } from './data';
import { doctorChecks, quarantineRows, verifyChain } from './system';

const HOUR = 3_600_000;
const DAY = 86_400_000;

type Q = Partial<Query>;

export function deltaPct(current: number, previous: number): number | null {
  return Math.abs(previous) > 1e-9 ? ((current - previous) / previous) * 100 : null;
}

function between(records: MockRecord[], from: number, to: number): MockRecord[] {
  return records.filter((r) => r.ts >= from && r.ts < to);
}

export function filtered(ds: Dataset, q: Q, defaultDays = 30): { from: number; to: number; records: MockRecord[] } {
  const now = ds.clock();
  const to = q.to ?? now + 1;
  const from = q.from ?? startOfDay(now) - (defaultDays - 1) * DAY;
  const records = ds.records.filter(
    (r) =>
      r.ts >= from &&
      r.ts < to &&
      (q.project == null || r.project === Number(q.project)) &&
      (q.model == null || r.model === q.model) &&
      (q.agent == null || (r.agent ?? 'main') === q.agent) &&
      (q.client == null || clientOf(ds, r.project) === q.client),
  );
  return { from, to, records };
}

function clientOf(ds: Dataset, project: number): string | null {
  return ds.projects.find((p) => p.id === project)?.client ?? null;
}

export function projectLabel(ds: Dataset, id: number): string {
  if (ds.settings.privacy.redact_paths) return `Project ${id}`;
  return ds.projects.find((p) => p.id === id)?.name ?? `Project ${id}`;
}

// ------------------------------------------------------------------ buckets

export function weekStartOf(ts: number, weekStart: number): number {
  const d = new Date(startOfDay(ts));
  const dow = (d.getDay() + 6) % 7;
  d.setDate(d.getDate() - ((dow - weekStart + 7) % 7));
  return d.getTime();
}

export function bucketStart(ts: number, g: Granularity, weekStart: number): number {
  const d = new Date(ts);
  switch (g) {
    case 'hour':
      d.setMinutes(0, 0, 0);
      return d.getTime();
    case 'day':
      return startOfDay(ts);
    case 'week':
      return weekStartOf(ts, weekStart);
    case 'month':
      return new Date(d.getFullYear(), d.getMonth(), 1).getTime();
  }
}

function nextBucket(start: number, g: Granularity): number {
  const d = new Date(start);
  switch (g) {
    case 'hour':
      return start + HOUR;
    case 'day':
      d.setDate(d.getDate() + 1);
      return d.getTime();
    case 'week':
      d.setDate(d.getDate() + 7);
      return d.getTime();
    case 'month':
      return new Date(d.getFullYear(), d.getMonth() + 1, 1).getTime();
  }
}

const pad = (n: number) => String(n).padStart(2, '0');

function isoWeek(ts: number): string {
  const d = new Date(ts);
  const target = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = (target.getUTCDay() + 6) % 7;
  target.setUTCDate(target.getUTCDate() - day + 3);
  const firstThursday = new Date(Date.UTC(target.getUTCFullYear(), 0, 4));
  const week = 1 + Math.round(((target.getTime() - firstThursday.getTime()) / DAY - 3 + ((firstThursday.getUTCDay() + 6) % 7)) / 7);
  return `${target.getUTCFullYear()}-W${pad(week)}`;
}

export function bucketLabel(start: number, g: Granularity): string {
  const d = new Date(start);
  const ymd = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  switch (g) {
    case 'hour':
      return `${ymd} ${pad(d.getHours())}:00`;
    case 'day':
      return ymd;
    case 'week':
      return isoWeek(start);
    case 'month':
      return `${d.getFullYear()}-${pad(d.getMonth() + 1)}`;
  }
}

function slices(records: MockRecord[]): ModelSlice[] {
  const m = new Map<string, ModelSlice>();
  for (const r of records) {
    const s = m.get(r.model) ?? { model: r.model, cost_usd: 0, tokens: 0 };
    s.cost_usd += r.cost ?? 0;
    s.tokens += totalTokens(r.tokens);
    m.set(r.model, s);
  }
  return [...m.values()].sort((a, b) => b.cost_usd - a.cost_usd);
}

export function buckets(records: MockRecord[], from: number, to: number, g: Granularity, weekStart: number): UsageBucket[] {
  const out: UsageBucket[] = [];
  const groups = new Map<number, MockRecord[]>();
  for (const r of records) {
    const b = bucketStart(r.ts, g, weekStart);
    const list = groups.get(b);
    if (list) list.push(r);
    else groups.set(b, [r]);
  }
  for (let s = bucketStart(from, g, weekStart); s < to; s = nextBucket(s, g)) {
    const list = groups.get(s) ?? [];
    out.push({ start: s, label: bucketLabel(s, g), usage: sumUsage(list), by_model: slices(list) });
  }
  return out;
}

export function usageReport(ds: Dataset, q: Q): UsageReport {
  const { from, to, records } = filtered(ds, q);
  const group = q.group ?? 'day';
  return { group, from, to, buckets: buckets(records, from, to, group, ds.settings.week_start), total: sumUsage(records), confidence: 'lower_bound' };
}

// ---------------------------------------------------------------- breakdown

function rowKey(ds: Dataset, r: MockRecord, by: Dimension, sessions: Map<string, MockSession>): [string, string][] {
  switch (by) {
    case 'project':
      return [[String(r.project), projectLabel(ds, r.project)]];
    case 'model':
      return [[r.model, r.model]];
    case 'agent':
      return [[r.agent ?? 'main', r.agent ?? 'main']];
    case 'client': {
      const c = clientOf(ds, r.project);
      return [[c ?? '', c ?? '']];
    }
    case 'branch': {
      const b = sessions.get(r.session)?.branch ?? '';
      return [[b, b]];
    }
    case 'session': {
      const s = sessions.get(r.session);
      return [[r.session, ds.settings.privacy.session_titles && s ? s.title : r.session.slice(0, 12)]];
    }
    case 'tool':
      return [...new Set(r.tools)].map((t) => [t, t]);
    case 'skill':
      return r.skill ? [[r.skill, r.skill]] : [];
    case 'mcp':
      return r.mcp ? [[r.mcp, r.mcp]] : [];
  }
}

export function breakdownRows(ds: Dataset, records: MockRecord[], by: Dimension): BreakdownRow[] {
  const sessions = new Map(ds.sessions.map((s) => [s.id, s]));
  const rows = new Map<string, BreakdownRow>();
  const total = sumUsage(records).cost_usd;
  for (const r of records) {
    const keys = rowKey(ds, r, by, sessions);
    for (const [key, label] of keys) {
      const row = rows.get(key) ?? { key, label, usage: emptyUsage(), share: 0 };
      addRecord(row.usage, r);
      rows.set(key, row);
    }
  }
  return [...rows.values()]
    .map((row) => ({ ...row, share: total > 0 ? row.usage.cost_usd / total : 0 }))
    .sort((a, b) => b.usage.cost_usd - a.usage.cost_usd);
}

export function breakdown(ds: Dataset, q: Q): Breakdown {
  const by = q.by ?? 'project';
  const { from, to, records } = filtered(ds, q);
  const confidence: Confidence = by === 'skill' || by === 'mcp' || by === 'tool' ? 'estimate' : 'lower_bound';
  return { by, from, to, rows: breakdownRows(ds, records, by), total: sumUsage(records), confidence };
}

// ------------------------------------------------------------------- blocks

export function computeBlocks(ds: Dataset, records: MockRecord[] = ds.records): Block[] {
  const now = ds.clock();
  const out: { start: number; end: number; recs: MockRecord[] }[] = [];
  for (const r of records) {
    const cur = out[out.length - 1];
    if (!cur || r.ts >= cur.end) {
      const s = new Date(r.ts);
      s.setMinutes(0, 0, 0);
      out.push({ start: s.getTime(), end: s.getTime() + 5 * HOUR, recs: [r] });
    } else cur.recs.push(r);
  }
  const burn = burnRate(ds);
  return out.map((b) => {
    const usage = sumUsage(b.recs);
    const isActive = now >= b.start && now < b.end;
    const remainingH = Math.max(0, (b.end - now) / HOUR);
    return {
      start: b.start,
      end: b.end,
      is_active: isActive,
      anchored: isAnchored(ds, b.start, b.end),
      peak_pct: isAnchored(ds, b.start, b.end) ? Math.min(100, (usage.cost_usd / ds.fiveHourCeilingUsd) * 100) : null,
      usage,
      by_model: breakdownRows(ds, b.recs, 'model'),
      by_project: breakdownRows(ds, b.recs, 'project'),
      burn_usd_per_hour: isActive ? burn.usd_per_hour : usage.cost_usd / Math.max(0.25, (b.recs[b.recs.length - 1].ts - b.start) / HOUR),
      projected_usd: isActive ? usage.cost_usd + burn.usd_per_hour * remainingH : null,
    };
  });
}

export function burnRate(ds: Dataset): BurnRate {
  const now = ds.clock();
  const window = 60;
  const recs = between(ds.records, now - window * 60_000, now + 1);
  const u = sumUsage(recs);
  return { usd_per_hour: u.cost_usd * (60 / window), tokens_per_min: totalTokens(u.tokens) / window, window_minutes: window, active: recs.length > 0 };
}

// ------------------------------------------------------------------- limits

function weeklyWindow(ds: Dataset, ts: number): { start: number; end: number } {
  let end = ds.weeklyResetsAt;
  while (end - 7 * DAY > ts) end -= 7 * DAY;
  while (end <= ts) end += 7 * DAY;
  return { start: end - 7 * DAY, end };
}

/** Past weeks bought a little less per percentage point; the current week is the calibrated one. */
function weekFactor(ds: Dataset, end: number): number {
  const k = Math.round((weeklyWindow(ds, ds.clock()).end - end) / (7 * DAY));
  if (k <= 0) return 1;
  const usd = between(ds.records, end - 7 * DAY, end).reduce((a, r) => a + (r.cost ?? 0), 0);
  return Math.max(0.5, usd / (ds.weeklyCeilingUsd * (0.8 + 0.13 * Math.sin(k * 2.1))));
}

/** Windows with a statusline reading are anchored to its reset; headless and weekend runs had none. */
function isAnchored(ds: Dataset, start: number, end: number): boolean {
  if (ds.statuslineSince === null || end <= ds.statuslineSince) return false;
  const d = new Date(start);
  return start + 5 * HOUR > ds.clock() || (d.getDay() !== 0 && d.getDay() !== 6 && d.getHours() >= 9);
}

function weeklyPctAt(ds: Dataset, ts: number): { pct: number; usd: number; end: number } {
  const w = weeklyWindow(ds, ts);
  const usd = between(ds.records, w.start, ts + 1).reduce((a, r) => a + (r.cost ?? 0), 0);
  return { pct: Math.min(100, (usd / (ds.weeklyCeilingUsd * weekFactor(ds, w.end))) * 100), usd, end: w.end };
}

function spendLimit(ds: Dataset): SpendLimit | null {
  if (ds.spendLimitUsd === null) return null;
  const now = ds.clock();
  const start = bucketStart(now, 'month', 0);
  const d = new Date(start);
  const end = new Date(d.getFullYear(), d.getMonth() + 1, 1).getTime();
  const used = ds.spendLimitUsd * 0.71;
  const ex = now + (ds.spendLimitUsd - used) / (used / Math.max(HOUR, now - start));
  return {
    used_pct: (used / ds.spendLimitUsd) * 100,
    resets_at: end,
    observed_at: ds.hooks.last_statusline_at ?? now,
    used_usd: used,
    limit_usd: ds.spendLimitUsd,
    period: 'month',
    exhausts_at: ex < end ? ex : null,
    will_exhaust: ex < end,
  };
}

export function limits(ds: Dataset): Limits {
  const now = ds.clock();
  if (!ds.records.length) return { five_hour: null, seven_day: null, spend: null, estimate: null };
  const blocks = computeBlocks(ds);
  const active = blocks.find((b) => b.is_active) ?? null;
  if (!ds.hooks.statusline_installed) {
    const past = blocks.filter((b) => !b.is_active).map((b) => b.usage.cost_usd).sort((a, b) => a - b);
    if (past.length < 5) return { five_hour: null, seven_day: null, spend: null, estimate: null };
    const p90 = past[Math.min(past.length - 1, Math.floor(past.length * 0.9))];
    const cur = active?.usage.cost_usd ?? 0;
    return {
      five_hour: null,
      seven_day: null,
      spend: null,
      estimate: { p90_block_usd: p90, blocks_observed: past.length, current_block_usd: cur, current_block_pct: (cur / p90) * 100, confidence: 'estimate' },
    };
  }
  const observed = ds.hooks.last_statusline_at ?? now;
  const burn = burnRate(ds);
  let five_hour: Limits['five_hour'] = null;
  if (active) {
    const used = Math.min(100, (active.usage.cost_usd / ds.fiveHourCeilingUsd) * 100);
    const rate = (burn.usd_per_hour / ds.fiveHourCeilingUsd) * 100;
    const ex = rate > 0 ? now + ((100 - used) / rate) * HOUR : null;
    five_hour = {
      used_pct: used,
      resets_at: active.end,
      observed_at: observed,
      stale: now - observed > 15 * 60_000,
      rate_pct_per_hour: rate,
      exhausts_at: ex !== null && ex < active.end ? ex : null,
      will_exhaust: ex !== null && ex < active.end,
      usd_in_window: active.usage.cost_usd,
      usd_per_pct: ds.fiveHourCeilingUsd / 100,
      daily_budget_pct: null,
      confidence: 'official',
    };
  }
  const wk = weeklyPctAt(ds, now);
  const w = weeklyWindow(ds, now);
  const lookback = Math.max(HOUR, Math.min(now - w.start, 3 * DAY));
  const recentUsd = between(ds.records, now - lookback, now + 1).reduce((a, r) => a + (r.cost ?? 0), 0);
  const rate7 = (recentUsd / (lookback / HOUR) / ds.weeklyCeilingUsd) * 100;
  const ex7 = rate7 > 0 ? now + ((100 - wk.pct) / rate7) * HOUR : null;
  const daysLeft = Math.max(1 / 24, (w.end - now) / DAY);
  return {
    five_hour,
    seven_day: {
      used_pct: wk.pct,
      resets_at: w.end,
      observed_at: observed,
      stale: now - observed > 15 * 60_000,
      rate_pct_per_hour: rate7,
      exhausts_at: ex7 !== null && ex7 < w.end ? ex7 : null,
      will_exhaust: ex7 !== null && ex7 < w.end,
      usd_in_window: wk.usd,
      usd_per_pct: ds.weeklyCeilingUsd / 100,
      daily_budget_pct: Math.max(0, (100 - wk.pct) / daysLeft),
      confidence: 'official',
    },
    spend: spendLimit(ds),
    estimate: null,
  };
}

export function limitsDetail(ds: Dataset): LimitsDetail {
  const now = ds.clock();
  const history_7d: LimitPoint[] = [];
  const history_5h: LimitPoint[] = [];
  if (ds.hooks.statusline_installed && ds.records.length) {
    for (let ts = startOfDay(now) - 13 * DAY; ts <= now; ts += 2 * HOUR) {
      const p = weeklyPctAt(ds, ts);
      history_7d.push({ ts, used_pct: p.pct, resets_at: p.end });
    }
    for (const b of computeBlocks(ds).filter((b) => b.end > now - 2 * DAY)) {
      for (let ts = b.start; ts <= Math.min(b.end, now); ts += 30 * 60_000) {
        const usd = between(ds.records, b.start, ts + 1).reduce((a, r) => a + (r.cost ?? 0), 0);
        history_5h.push({ ts, used_pct: Math.min(100, (usd / ds.fiveHourCeilingUsd) * 100), resets_at: b.end });
      }
    }
  }
  const usd_per_pct_weekly: UsdPerPctPoint[] = [];
  const current = weeklyWindow(ds, now);
  for (let k = 7; k >= 0; k--) {
    const start = current.start - k * 7 * DAY;
    const usd = between(ds.records, start, start + 7 * DAY).reduce((a, r) => a + (r.cost ?? 0), 0);
    const read = ds.statuslineSince !== null && start + 7 * DAY > ds.statuslineSince;
    const pct = read ? Math.min(100, (usd / (ds.weeklyCeilingUsd * weekFactor(ds, start + 7 * DAY))) * 100) : 0;
    usd_per_pct_weekly.push({ week_start: start, usd, pct, usd_per_pct: pct > 0 ? usd / pct : null });
  }
  return { current: limits(ds), history_5h, history_7d, usd_per_pct_weekly, blocks: computeBlocks(ds).slice(-40).reverse() };
}

// ----------------------------------------------------------------- overview

function period(ds: Dataset, from: number, to: number, prevFrom: number, prevTo: number): PeriodSummary {
  const usage = sumUsage(between(ds.records, from, to));
  const previous = sumUsage(between(ds.records, prevFrom, prevTo));
  return { from, to, usage, previous, delta_pct: deltaPct(usage.cost_usd, previous.cost_usd) };
}

export function sessionSummary(ds: Dataset, s: MockSession, records?: MockRecord[]): SessionSummary {
  const recs = records ?? ds.records.filter((r) => r.session === s.id);
  const usage = sumUsage(recs);
  const models = [...new Set(recs.map((r) => r.model))];
  const main = recs.filter((r) => r.agent === null);
  const byModel = new Map<string, number>();
  for (const r of main) byModel.set(r.model, (byModel.get(r.model) ?? 0) + r.requests);
  const mainModel = [...byModel.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
  const t = usage.tokens;
  const denom = t.cache_read + t.cache_write_5m + t.cache_write_1h + t.input;
  const now = ds.clock();
  return {
    id: s.id,
    project_id: s.project,
    project: projectLabel(ds, s.project),
    title: ds.settings.privacy.session_titles ? s.title : null,
    git_branch: s.branch,
    started_at: s.started,
    last_at: s.last,
    usage,
    main_model: mainModel,
    models,
    is_active: s.last >= now - 10 * 60_000,
    subagent_cost_usd: recs.filter((r) => r.agent !== null).reduce((a, r) => a + (r.cost ?? 0), 0),
    context_pct: s.ctxPct,
    cache_hit_ratio: denom > 0 ? t.cache_read / denom : null,
  };
}

function bySession(ds: Dataset): Map<string, MockRecord[]> {
  const m = new Map<string, MockRecord[]>();
  for (const r of ds.records) {
    const l = m.get(r.session);
    if (l) l.push(r);
    else m.set(r.session, [r]);
  }
  return m;
}

export function dataStatus(ds: Dataset): DataStatus {
  const unpriced = [...new Set(ds.records.filter((r) => r.cost === null).map((r) => r.model))];
  return {
    statusline_connected: ds.hooks.statusline_installed,
    last_statusline_at: ds.hooks.last_statusline_at,
    last_request_at: ds.records.length ? ds.records[ds.records.length - 1].ts : null,
    first_request_at: ds.records.length ? ds.records[0].ts : null,
    unpriced_models: unpriced,
    quarantined_records: ds.scenario === 'unpriced' ? 3 : 0,
    cost_confidence: 'lower_bound',
  };
}

export function overview(ds: Dataset): Overview {
  const now = ds.clock();
  const today = startOfDay(now);
  const yesterday = today - DAY;
  const week = weekStartOf(now, ds.settings.week_start);
  const d = new Date(now);
  const month = new Date(d.getFullYear(), d.getMonth(), 1).getTime();
  const prevMonth = new Date(d.getFullYear(), d.getMonth() - 1, 1).getTime();
  const end = now + 1;
  const blocks = computeBlocks(ds);
  const grouped = bySession(ds);
  const last30 = between(ds.records, today - 29 * DAY, end);
  const hourStart = bucketStart(now, 'hour', 0) - 23 * HOUR;
  return {
    generated_at: now,
    today: period(ds, today, end, yesterday, now - DAY + 1),
    week: period(ds, week, end, week - 7 * DAY, now - 7 * DAY + 1),
    month: period(ds, month, end, prevMonth, Math.min(prevMonth + (now - month) + 1, month)),
    burn: burnRate(ds),
    limits: limits(ds),
    active_block: blocks.find((b) => b.is_active) ?? null,
    active_sessions: ds.sessions
      .filter((s) => s.last >= now - 10 * 60_000)
      .map((s) => sessionSummary(ds, s, grouped.get(s.id) ?? []))
      .sort((a, b) => b.last_at - a.last_at),
    top_projects: breakdownRows(ds, last30, 'project').slice(0, 6),
    top_models: breakdownRows(ds, last30, 'model').slice(0, 6),
    hourly: buckets(between(ds.records, hourStart, end), hourStart, end, 'hour', 0),
    daily: buckets(last30, today - 29 * DAY, end, 'day', ds.settings.week_start),
    data: dataStatus(ds),
  };
}

// ----------------------------------------------------------------- sessions

export function sessionList(ds: Dataset, q: Q): SessionList {
  const grouped = bySession(ds);
  const needle = q.q?.toLowerCase().trim();
  const from = q.from ?? 0;
  const to = q.to ?? Number.MAX_SAFE_INTEGER;
  let items = ds.sessions
    .filter((s) => s.last >= from && s.started < to && (q.project == null || s.project === Number(q.project)))
    .map((s) => sessionSummary(ds, s, grouped.get(s.id) ?? []))
    .filter((s) => s.usage.requests > 0)
    .filter((s) => !needle || [s.project, s.git_branch ?? '', s.title ?? '', s.id].some((v) => v.toLowerCase().includes(needle)));
  const sort = q.sort ?? 'cost';
  const val = (s: SessionSummary) =>
    sort === 'recent' ? s.last_at : sort === 'tokens' ? totalTokens(s.usage.tokens) : sort === 'duration' ? s.last_at - s.started_at : s.usage.cost_usd;
  items.sort((a, b) => val(b) - val(a));
  const total = items.length;
  const offset = q.offset ?? 0;
  items = items.slice(offset, offset + (q.limit ?? 50));
  return { items, total };
}

function turn(ds: Dataset, r: MockRecord): Turn {
  return {
    message_id: r.id,
    session_id: r.session,
    project: projectLabel(ds, r.project),
    ts: r.ts,
    model: r.model,
    tokens: r.tokens,
    cost_usd: r.cost,
    agent_type: r.agent,
    tools: r.tools,
    stop_reason: r.agent !== null ? 'end_turn' : ['tool_use', 'tool_use', 'tool_use', 'end_turn', 'tool_use', 'end_turn', 'tool_use', 'max_tokens'][r.id.charCodeAt(r.id.length - 1) % 8],
  };
}

export function turns(ds: Dataset, q: Q): Turn[] {
  const { records } = filtered(ds, q);
  return [...records]
    .sort((a, b) => (b.cost ?? 0) - (a.cost ?? 0))
    .slice(0, q.limit ?? 20)
    .map((r) => turn(ds, r));
}

function seeded(id: string): () => number {
  let h = 2166136261;
  for (let i = 0; i < id.length; i++) h = Math.imul(h ^ id.charCodeAt(i), 16777619);
  let a = h >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** 5-minute buckets for sessions up to 2 hours, hourly up to 2 days, daily beyond (as the server does). */
function sessionTimeline(recs: MockRecord[], from: number, to: number): UsageBucket[] {
  const span = to - from;
  if (span > 2 * DAY) return buckets(recs, from, to, 'day', 0);
  if (span > 2 * HOUR) return buckets(recs, from, to, 'hour', 0);
  const step = 5 * 60_000;
  const first = bucketStart(from, 'hour', 0) + Math.floor((from - bucketStart(from, 'hour', 0)) / step) * step;
  const out: UsageBucket[] = [];
  for (let s = first; s < to; s += step) {
    const list = recs.filter((r) => r.ts >= s && r.ts < s + step);
    const d = new Date(s);
    const label = `${bucketLabel(s, 'day')} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
    out.push({ start: s, label, usage: sumUsage(list), by_model: slices(list) });
  }
  return out;
}

const MISS_CAUSES = ['ttl_expired_5m', 'ttl_expired_5m', 'ttl_expired_5m', 'tools_changed', 'tools_changed', 'system_changed', 'claude_md_changed', 'model_changed'];

function sessionMisses(ds: Dataset, s: MockSession, recs: MockRecord[]): CacheMiss[] {
  const rand = seeded(`miss-${s.id}`);
  const main = recs.filter((r) => r.agent === null && r.ts > s.started);
  const n = main.length < 4 ? 0 : Math.floor(rand() * 4);
  const picked = new Set<number>();
  for (let i = 0; i < n; i++) picked.add(1 + Math.floor(rand() * (main.length - 1)));
  const project = projectLabel(ds, s.project);
  return [...picked]
    .sort((a, b) => a - b)
    .map((i) => {
      const ts = main[i].ts;
      const cause = MISS_CAUSES[Math.floor(rand() * MISS_CAUSES.length)];
      const ago = (m: number) => ts - Math.round(m * 60_000);
      const base: CacheMiss = { session_id: s.id, project, ts, causes: [cause], tools_added: null, tools_removed: null, system_char_delta: null, correlated_changes: [] };
      if (cause === 'tools_changed') {
        const added = 1 + Math.floor(rand() * 6);
        return { ...base, tools_added: added, tools_removed: rand() < 0.4 ? 1 : 0, correlated_changes: [{ ts: ago(2 + rand() * 6), kind: 'plugins', target: rand() < 0.5 ? 'issue-tracker' : 'docs-search' }] };
      }
      if (cause === 'system_changed') return { ...base, system_char_delta: Math.round(400 + rand() * 2600), correlated_changes: [{ ts: ago(1 + rand() * 4), kind: 'settings', target: '.claude/settings.json' }] };
      if (cause === 'claude_md_changed')
        return { ...base, causes: ['claude_md_changed', 'system_changed'], system_char_delta: Math.round(120 + rand() * 900), correlated_changes: [{ ts: ago(1 + rand() * 8), kind: 'claude_md', target: 'CLAUDE.md' }] };
      return base;
    });
}

function sessionEvents(s: MockSession, active: boolean): SessionEvent[] {
  const rand = seeded(`events-${s.id}`);
  const events: SessionEvent[] = [{ ts: s.started, event: 'SessionStart', source: rand() < 0.75 ? 'startup' : 'resume' }];
  for (let ts = s.started + 18 * 60_000; ts < s.last - 4 * 60_000; ts += Math.round((14 + rand() * 30) * 60_000)) events.push({ ts, event: 'Stop', source: null });
  if (!active) events.push({ ts: s.last, event: 'SessionEnd', source: rand() < 0.7 ? 'prompt_input_exit' : 'clear' });
  return events;
}

export function sessionDetail(ds: Dataset, id: string): SessionDetail | undefined {
  const s = ds.sessions.find((x) => x.id === id);
  if (!s) return undefined;
  const recs = ds.records.filter((r) => r.session === id);
  const summary = sessionSummary(ds, s, recs);
  const now = ds.clock();
  const lastHour = recs.filter((r) => r.ts >= now - HOUR);
  const u = sumUsage(lastHour);
  const rand = seeded(`status-${s.id}`);
  const misses = sessionMisses(ds, s, recs);
  const causes: Record<string, number> = {};
  for (const m of misses) for (const c of m.causes.slice(0, 1)) causes[c] = (causes[c] ?? 0) + 1;
  const t = summary.usage.tokens;
  const ctxTokens = Math.round(s.ctxPct * 2000);
  const hasStatus = ds.hooks.statusline_installed && (summary.is_active || s.last >= now - 21 * DAY);
  const warm = now - s.last < 5 * 60_000;
  const cache: CacheStatus = {
    warm,
    ttl: '5m',
    expires_at: s.last + 5 * 60_000,
    requests: summary.usage.requests,
    misses: misses.length,
    expected_rebuilds: Math.max(0, Math.round((s.last - s.started) / (45 * 60_000))),
    hit_ratio: summary.cache_hit_ratio,
    write_tokens: t.cache_write_5m + t.cache_write_1h,
    miss_recache_tokens: misses.length * Math.round(ctxTokens * 0.8),
    last_miss_at: misses.length ? misses[misses.length - 1].ts : null,
    miss_causes: causes,
    recache_tokens_if_cold: ctxTokens,
  };
  return {
    summary,
    timeline: sessionTimeline(recs, s.started, s.last + 1),
    by_model: breakdownRows(ds, recs, 'model'),
    by_agent: breakdownRows(ds, recs, 'agent'),
    tools: breakdownRows(ds, recs, 'tool'),
    top_turns: [...recs].sort((a, b) => (b.cost ?? 0) - (a.cost ?? 0)).slice(0, 10).map((r) => turn(ds, r)),
    status: hasStatus
      ? {
          ts: summary.is_active ? s.last : s.last - 30_000,
          model: summary.main_model,
          cost_usd: summary.usage.cost_usd * (1.01 + rand() * 0.06) + summary.usage.unpriced_requests * 0.04,
          duration_ms: s.last - s.started,
          api_duration_ms: Math.round((s.last - s.started) * (0.35 + rand() * 0.2)),
          lines_added: Math.round(40 + rand() * 600),
          lines_removed: Math.round(10 + rand() * 220),
          ctx_used_pct: s.ctxPct,
          ctx_size: 200_000,
          exceeds_200k: false,
          cache,
        }
      : null,
    cache_misses: misses,
    events: sessionEvents(s, summary.is_active),
    burn: { usd_per_hour: u.cost_usd, tokens_per_min: totalTokens(u.tokens) / 60, window_minutes: 60, active: lastHour.length > 0 },
  };
}

const TRANSCRIPT_FILES = ['src/billing/refunds.ts', 'src/webhooks/sender.ts', 'src/auth/middleware.ts', 'tests/checkout.spec.ts', 'docs/reference/orders.md'];

export function transcript(ds: Dataset, id: string): Transcript {
  const s = ds.sessions.find((x) => x.id === id);
  const rand = seeded(`transcript-${id}`);
  const start = s?.started ?? ds.clock();
  const file = TRANSCRIPT_FILES[Math.floor(rand() * TRANSCRIPT_FILES.length)];
  const task = (s?.title ?? 'Tidy up error messages').toLowerCase();
  const model = 'claude-opus-4-5';
  const at = (m: number) => start + Math.round(m * 60_000);
  return {
    session_id: id,
    truncated: false,
    messages: [
      { ts: at(0), role: 'user', kind: 'text', text: `Sample conversation (invented). Please ${task}. Start with ${file} and keep the public interface unchanged.`, model: null, tool: null },
      { ts: at(0.3), role: 'assistant', kind: 'thinking', text: `The request is to ${task}. I should read ${file} first and check which tests cover it before changing anything.`, model, tool: null },
      { ts: at(0.5), role: 'assistant', kind: 'tool_use', text: `Read ${file}`, model, tool: 'Read' },
      { ts: at(0.6), role: 'user', kind: 'tool_result', text: `1  export function apply(input: Order): Result {\n2    const total = sum(input.lines);\n3    return round(total, 2);\n4  }`, model: null, tool: 'Read' },
      { ts: at(1.4), role: 'assistant', kind: 'text', text: 'The rounding happens before the refund lines are subtracted, so a refund can be off by one cent. I will move the rounding to the end and add a test for it.', model, tool: null },
      { ts: at(2.1), role: 'assistant', kind: 'tool_use', text: `Edit ${file}`, model, tool: 'Edit' },
      { ts: at(2.2), role: 'user', kind: 'tool_result', text: 'The file was updated.', model: null, tool: 'Edit' },
      { ts: at(3), role: 'assistant', kind: 'tool_use', text: 'npm test -- refunds', model, tool: 'Bash' },
      { ts: at(3.8), role: 'user', kind: 'tool_result', text: 'Tests: 14 passed, 14 total', model: null, tool: 'Bash' },
      { ts: at(4.2), role: 'assistant', kind: 'text', text: 'Done. The rounding now happens once, after refunds, and a new test covers the one-cent case.', model, tool: null },
      { ts: at(5), role: 'user', kind: 'text', text: 'Thanks. Please also update the changelog.', model: null, tool: null },
    ],
  };
}

// ----------------------------------------------------------------- projects

export function projects(ds: Dataset): Project[] {
  const now = ds.clock();
  const since = startOfDay(now) - 29 * DAY;
  return ds.projects
    .map((p) => {
      const recs = ds.records.filter((r) => r.project === p.id);
      const sessions = ds.sessions.filter((s) => s.project === p.id && recs.some((r) => r.session === s.id));
      return {
        id: p.id,
        name: projectLabel(ds, p.id),
        path: ds.settings.privacy.redact_paths ? null : p.path,
        client: p.client,
        first_seen: recs[0]?.ts ?? ds.createdAt,
        last_seen: recs[recs.length - 1]?.ts ?? ds.createdAt,
        sessions: sessions.length,
        usage_30d: sumUsage(recs.filter((r) => r.ts >= since)),
        usage_total: sumUsage(recs),
      };
    })
    .filter((p) => p.usage_total.requests > 0)
    .sort((a, b) => b.usage_30d.cost_usd - a.usage_30d.cost_usd);
}

// ------------------------------------------------------------------ heatmap

export function heatmap(ds: Dataset, q: Q): Heatmap {
  const { from, to, records } = filtered(ds, { ...q, from: q.from ?? startOfDay(ds.clock()) - 364 * DAY }, 365);
  const cells = new Map<string, { dow: number; hour: number; cost_usd: number; tokens: number; requests: number }>();
  for (let dow = 0; dow < 7; dow++) for (let hour = 0; hour < 24; hour++) cells.set(`${dow}-${hour}`, { dow, hour, cost_usd: 0, tokens: 0, requests: 0 });
  const days = new Map<string, { date: string; cost_usd: number; tokens: number; requests: number }>();
  for (let s = startOfDay(from); s < to; s = startOfDay(s + DAY + HOUR)) {
    const label = bucketLabel(s, 'day');
    days.set(label, { date: label, cost_usd: 0, tokens: 0, requests: 0 });
  }
  for (const r of records) {
    const d = new Date(r.ts);
    const c = cells.get(`${(d.getDay() + 6) % 7}-${d.getHours()}`);
    const day = days.get(bucketLabel(startOfDay(r.ts), 'day'));
    for (const x of [c, day]) {
      if (!x) continue;
      x.cost_usd += r.cost ?? 0;
      x.tokens += totalTokens(r.tokens);
      x.requests += r.requests;
    }
  }
  return { from, to, cells: [...cells.values()], days: [...days.values()] };
}

// ------------------------------------------------------------------ compare

export function compare(ds: Dataset, q: Q): Comparison {
  const now = ds.clock();
  const periodName = q.period === 'month' ? 'month' : 'week';
  const offset = Number(q.offset ?? 0);
  let curFrom: number;
  let curTo: number;
  let prevFrom: number;
  let prevTo: number;
  if (periodName === 'week') {
    const w = weekStartOf(now, ds.settings.week_start) - offset * 7 * DAY;
    curFrom = w;
    curTo = offset === 0 ? now + 1 : w + 7 * DAY;
    prevFrom = w - 7 * DAY;
    prevTo = curTo - 7 * DAY;
  } else {
    const d = new Date(now);
    curFrom = new Date(d.getFullYear(), d.getMonth() - offset, 1).getTime();
    const next = new Date(d.getFullYear(), d.getMonth() - offset + 1, 1).getTime();
    curTo = offset === 0 ? now + 1 : next;
    prevFrom = new Date(d.getFullYear(), d.getMonth() - offset - 1, 1).getTime();
    prevTo = Math.min(prevFrom + (curTo - curFrom), curFrom);
  }
  const cur = between(ds.records, curFrom, curTo);
  const prev = between(ds.records, prevFrom, prevTo);
  const drivers: Driver[] = [];
  for (const dim of ['project', 'model', 'agent'] as const) {
    const a = new Map(breakdownRows(ds, cur, dim).map((r) => [r.key, r]));
    const b = new Map(breakdownRows(ds, prev, dim).map((r) => [r.key, r]));
    for (const key of new Set([...a.keys(), ...b.keys()])) {
      const c = a.get(key)?.usage.cost_usd ?? 0;
      const p = b.get(key)?.usage.cost_usd ?? 0;
      drivers.push({ dimension: dim, key, label: a.get(key)?.label ?? b.get(key)?.label ?? key, current_usd: c, previous_usd: p, delta_usd: c - p });
    }
  }
  drivers.sort((x, y) => Math.abs(y.delta_usd) - Math.abs(x.delta_usd));
  const cu = sumUsage(cur);
  const pu = sumUsage(prev);
  return {
    period: periodName,
    current: { from: curFrom, to: curTo, label: bucketLabel(curFrom, periodName), usage: cu },
    previous: { from: prevFrom, to: prevTo, label: bucketLabel(prevFrom, periodName), usage: pu },
    delta_usd: cu.cost_usd - pu.cost_usd,
    delta_pct: deltaPct(cu.cost_usd, pu.cost_usd),
    drivers: drivers.slice(0, 10),
  };
}

// --------------------------------------------------------------------- plan

const PLAN_PRICE: Record<string, number | null> = { pro: 20, max5x: 100, max20x: 200, team: 30, team_premium: 150 };

export function planFit(ds: Dataset): PlanFit {
  const now = ds.clock();
  const kind = ds.settings.plan.kind;
  const sub = ds.settings.plan.monthly_usd ?? PLAN_PRICE[kind] ?? null;
  const months = [];
  const d = new Date(now);
  for (let k = 5; k >= 0; k--) {
    const from = new Date(d.getFullYear(), d.getMonth() - k, 1).getTime();
    const to = new Date(d.getFullYear(), d.getMonth() - k + 1, 1).getTime();
    months.push({ month: bucketLabel(from, 'month'), api_equivalent_usd: sumUsage(between(ds.records, from, to)).cost_usd, subscription_usd: sub });
  }
  const since = now - 30 * DAY;
  const api = sumUsage(between(ds.records, since, now + 1)).cost_usd;
  const blocks = computeBlocks(ds).filter((b) => b.start >= since);
  const near = blocks.filter((b) => (b.peak_pct ?? 0) >= 90).length;
  const peaks = blocks.map((b) => b.peak_pct ?? 0);
  let action: PlanFit['action'] = 'keep';
  let reason = 'within_plan';
  let target: PlanFit['target'] = null;
  if (kind === 'unknown') {
    action = 'unknown';
    reason = 'no_plan';
  } else if (!ds.records.length) {
    action = 'unknown';
    reason = 'no_history';
  } else if (near >= 6) {
    reason = 'frequent_limit_hits';
    if (kind !== 'max20x') {
      action = 'upgrade';
      target = kind === 'max5x' ? 'max20x' : 'max5x';
    }
  } else if (sub !== null && api < sub * 0.6) {
    action = 'downgrade';
    reason = 'low_utilisation';
  }
  return {
    plan: ds.settings.plan,
    months,
    api_equivalent_usd: api,
    subscription_usd: sub,
    roi: sub ? api / sub : null,
    max_five_hour_pct: peaks.length ? Math.max(...peaks) : null,
    max_seven_day_pct: ds.hooks.statusline_installed ? Math.max(0, ...limitsDetail(ds).usd_per_pct_weekly.map((w) => w.pct)) : null,
    blocks_near_limit: near,
    action,
    target,
    reason,
    confidence: 'estimate',
  };
}

// ----------------------------------------------------------------- insights

function configChanges(ds: Dataset): ConfigChange[] {
  const now = ds.clock();
  const quill = [...ds.sessions].reverse().find((s) => s.project === 3 && s.last < now - 2 * DAY);
  return [
    { ts: (quill?.started ?? now - 3 * DAY) - 25 * 60_000, kind: 'claude_md', target: 'quill-api/CLAUDE.md' },
    { ts: now - 6 * DAY - 3 * HOUR, kind: 'plugins', target: 'enabledPlugins' },
    { ts: now - 12 * DAY - 5 * HOUR, kind: 'tools', target: 'docs-search' },
  ];
}

function cacheBreakers(ds: Dataset): CacheBreaker[] {
  const [md, plugins, tools] = configChanges(ds);
  return [
    { change: md, misses_after: 9, sessions_affected: 3 },
    { change: plugins, misses_after: 5, sessions_affected: 2 },
    { change: tools, misses_after: 3, sessions_affected: 1 },
  ];
}

const RECENT_MISS_CAUSES = [['ttl_expired_5m'], ['tools_changed'], ['ttl_expired_5m'], ['model_changed'], ['ttl_expired_5m'], ['tools_changed', 'system_prompt_changed']];

function cacheMisses(ds: Dataset): CacheMiss[] {
  const now = ds.clock();
  const [md] = configChanges(ds);
  const quill = [...ds.sessions].reverse().find((s) => s.project === 3 && s.last < now - 2 * DAY);
  const recent = ds.sessions.filter((s) => s.last < now - 20 * 60_000 && s.last - s.started > 40 * 60_000).slice(-RECENT_MISS_CAUSES.length);
  const out: CacheMiss[] = recent.map((s, i) => {
    const causes = RECENT_MISS_CAUSES[i];
    return {
      session_id: s.id,
      project: projectLabel(ds, s.project),
      ts: s.started + Math.round((s.last - s.started) * 0.6),
      causes,
      tools_added: causes.includes('tools_changed') ? 2 : null,
      tools_removed: causes.includes('tools_changed') ? i % 2 : null,
      system_char_delta: causes.includes('system_prompt_changed') ? 1_240 : null,
      correlated_changes: [],
    };
  });
  if (quill)
    out.push({
      session_id: quill.id,
      project: projectLabel(ds, quill.project),
      ts: quill.started + 4 * 60_000,
      causes: ['system_prompt_changed'],
      tools_added: null,
      tools_removed: null,
      system_char_delta: 1_840,
      correlated_changes: [md],
    });
  return out.sort((a, b) => b.ts - a.ts).slice(0, 8);
}

function subagentMix(recent: MockRecord[]): ModelMixSuggestion[] {
  const subs = recent.filter((r) => r.agent === 'Explore' && r.model === 'claude-sonnet-4-5');
  if (!subs.length) return [];
  const current = subs.reduce((a, r) => a + (r.cost ?? 0), 0);
  const projected = subs.reduce((a, r) => a + (costOf('claude-haiku-4-5', r.tokens) ?? 0), 0);
  return [
    {
      code: 'subagents_to_haiku',
      from_model: 'claude-sonnet-4-5',
      to_model: 'claude-haiku-4-5',
      scope: 'Explore',
      affected_requests: subs.reduce((n, r) => n + r.requests, 0),
      current_usd: current,
      projected_usd: projected,
      saving_usd: current - projected,
      confidence: 'estimate',
    },
  ];
}

function sessionAnomalies(ds: Dataset): Anomaly[] {
  const now = ds.clock();
  const out: Anomaly[] = [];
  const alert = ds.alerts.find((a) => a.kind === 'runaway_session');
  const sid = (alert?.params as { session_id?: string } | null)?.session_id;
  const runaway = ds.sessions.find((s) => s.id === sid);
  if (runaway) {
    const cost = sumUsage(bySession(ds).get(runaway.id) ?? []).cost_usd;
    out.push({ kind: 'runaway_session', ts: runaway.last, session_id: runaway.id, project: projectLabel(ds, runaway.project), value: cost, baseline: cost / 3.1 });
  }
  const loop = [...ds.sessions].reverse().find((s) => s.project === 6 && s.last < now - DAY);
  if (loop) out.push({ kind: 'agent_loop', ts: loop.last, session_id: loop.id, project: projectLabel(ds, loop.project), value: 46, baseline: 30 });
  return out;
}

export function insights(ds: Dataset): Insights {
  const now = ds.clock();
  const recent = between(ds.records, now - 30 * DAY, now + 1);
  const total = sumUsage(recent).tokens;
  const ratio = (t: Usage['tokens']) => {
    const den = t.cache_read + t.cache_write_5m + t.cache_write_1h + t.input;
    return den > 0 ? t.cache_read / den : null;
  };
  const cache: CacheInsights = {
    hit_ratio: ratio(total),
    by_project: breakdownRows(ds, recent, 'project').map((r) => ({
      key: r.key,
      label: r.label,
      hit_ratio: ratio(r.usage.tokens),
      cache_read: r.usage.tokens.cache_read,
      cache_write: r.usage.tokens.cache_write_5m + r.usage.tokens.cache_write_1h,
      input: r.usage.tokens.input,
    })),
    miss_causes: recent.length
      ? [
          { cause: 'ttl_expired_5m', count: 14 },
          { cause: 'tools_changed', count: 6 },
          { cause: 'system_prompt_changed', count: 3 },
          { cause: 'model_changed', count: 2 },
        ]
      : [],
    recent_misses: recent.length ? cacheMisses(ds) : [],
    breakers: recent.length ? cacheBreakers(ds) : [],
  };
  const opusShort = recent.filter((r) => r.model === 'claude-opus-4-5' && r.agent === null && r.tokens.output < 4000);
  const current = opusShort.reduce((a, r) => a + (r.cost ?? 0), 0);
  const projected = current * 0.6;
  const daily = buckets(recent, startOfDay(now) - 29 * DAY, now + 1, 'day', ds.settings.week_start);
  const costs = daily.map((b) => b.usage.cost_usd).sort((a, b) => a - b);
  const median = costs[Math.floor(costs.length / 2)] ?? 0;
  const peak = daily.reduce((m, b) => (b.usage.cost_usd > m.usage.cost_usd ? b : m), daily[0]);
  const anomalies: Anomaly[] =
    peak && median > 0 && peak.usage.cost_usd > median * 1.8
      ? [{ kind: 'cost_spike', ts: peak.start, session_id: null, project: null, value: peak.usage.cost_usd, baseline: median }]
      : [];
  const grouped = bySession(ds);
  return {
    generated_at: now,
    cache,
    model_mix: [
      ...subagentMix(recent),
      ...(opusShort.length
      ? [
          {
            code: 'short_turns_to_sonnet',
            from_model: 'claude-opus-4-5',
            to_model: 'claude-sonnet-4-5',
            scope: 'main',
            affected_requests: opusShort.reduce((n, r) => n + r.requests, 0),
            current_usd: current,
            projected_usd: projected,
            saving_usd: current - projected,
            confidence: 'estimate' as const,
          },
        ]
      : []),
    ].sort((a, b) => b.saving_usd - a.saving_usd),
    context_warnings: ds.sessions
      .filter((s) => s.last >= now - 10 * 60_000 && s.ctxPct >= ds.settings.alerts.context_threshold)
      .map((s) => ({
        session_id: s.id,
        project: sessionSummary(ds, s, grouped.get(s.id) ?? []).project,
        ts: s.last,
        ctx_used_pct: s.ctxPct,
        ctx_size: 200_000,
        exceeds_200k: false,
      })),
    anomalies: [...anomalies, ...sessionAnomalies(ds)].sort((a, b) => b.ts - a.ts),
  };
}

// --------------------------------------------------------- alerts, budgets

const ALERT_TEXT: Record<string, Record<'en' | 'pl', [string, string]>> = {
  limit_forecast: {
    en: ['Weekly limit may run out before it resets', 'At the current pace the weekly limit is used up before the reset.'],
    pl: ['Limit tygodniowy może skończyć się przed odnowieniem', 'Przy obecnym tempie limit tygodniowy skończy się przed odnowieniem.'],
  },
  limit_threshold: {
    en: ['5-hour limit passed {threshold}%', 'The current 5-hour window has used {used_pct}% of its limit.'],
    pl: ['Limit 5-godzinny przekroczył {threshold}%', 'Bieżące okno 5-godzinne zużyło {used_pct}% limitu.'],
  },
  budget_warning: {
    en: ['Budget "{name}" is at {spent_pct}%', 'Spending this month is close to the amount you set.'],
    pl: ['Budżet „{name}” jest na poziomie {spent_pct}%', 'Wydatki w tym miesiącu zbliżają się do ustalonej kwoty.'],
  },
  context_near_limit: {
    en: ['A session is close to its context limit', 'One running session has used {ctx_used_pct}% of its context window. Claude Code will soon compact it.'],
    pl: ['Sesja zbliża się do limitu kontekstu', 'Jedna z trwających sesji zajęła {ctx_used_pct}% okna kontekstu. Claude Code wkrótce je skompresuje.'],
  },
  cache_miss_streak: {
    en: ['Cache misses in quill-api', 'The last {misses} requests could not reuse the cache, which makes them more expensive.'],
    pl: ['Chybienia pamięci podręcznej w quill-api', 'Ostatnie {misses} zapytania nie mogły skorzystać z pamięci podręcznej, przez co były droższe.'],
  },
  cost_spike: {
    en: ['Unusually expensive day', 'Spending was {factor} times the usual daily amount.'],
    pl: ['Wyjątkowo drogi dzień', 'Wydatki były {factor} razy wyższe niż zwykle w ciągu dnia.'],
  },
  runaway_session: {
    en: ['A session cost far more than usual', 'One session in {project} cost {factor} times as much as a typical session.'],
    pl: ['Sesja kosztowała znacznie więcej niż zwykle', 'Jedna sesja w projekcie {project} kosztowała {factor} razy więcej niż typowa sesja.'],
  },
  cache_cold: {
    en: ['The cache went cold', 'A session paused long enough for its cache to expire. The next request rebuilds it.'],
    pl: ['Pamięć podręczna wystygła', 'Sesja stała na tyle długo, że pamięć podręczna wygasła. Następne zapytanie odbuduje ją od nowa.'],
  },
};

export function renderAlert(a: Alert, lang: 'en' | 'pl'): Alert {
  const tpl = ALERT_TEXT[a.kind]?.[lang] ?? [a.kind, ''];
  const params = (a.params ?? {}) as Record<string, unknown>;
  const fill = (s: string) => s.replace(/\{(\w+)\}/g, (_m, k: string) => String(params[k] ?? ''));
  return { ...a, title: fill(tpl[0]), message: fill(tpl[1]) };
}

export function alertList(ds: Dataset, q: Q): AlertList {
  const lang = q.lang === 'pl' ? 'pl' : 'en';
  let items = ds.alerts;
  if (q.q === 'open') items = items.filter((a) => a.acknowledged_at === null);
  const offset = q.offset ?? 0;
  return {
    items: items.slice(offset, offset + (q.limit ?? 50)).map((a) => renderAlert(a, lang)),
    total: items.length,
    unacknowledged: ds.alerts.filter((a) => a.acknowledged_at === null).length,
  };
}

export function budgets(ds: Dataset): Budget[] {
  const now = ds.clock();
  return ds.budgets.map((b) => {
    const start = b.period === 'day' ? startOfDay(now) : b.period === 'week' ? weekStartOf(now, ds.settings.week_start) : bucketStart(now, 'month', 0);
    const end = b.period === 'day' ? start + DAY : b.period === 'week' ? start + 7 * DAY : new Date(new Date(start).getFullYear(), new Date(start).getMonth() + 1, 1).getTime();
    const recs = between(ds.records, start, now + 1).filter(
      (r) => (b.scope !== 'project' || String(r.project) === b.scope_value) && (b.scope !== 'client' || clientOf(ds, r.project) === b.scope_value),
    );
    const spent = sumUsage(recs).cost_usd;
    const elapsed = Math.max(0.02, (now - start) / (end - start));
    const pct = b.amount_usd > 0 ? (spent / b.amount_usd) * 100 : 0;
    return {
      ...b,
      scope_label: b.scope === 'project' && b.scope_value ? projectLabel(ds, Number(b.scope_value)) : b.scope_value,
      period_start: start,
      period_end: end,
      spent_usd: spent,
      spent_pct: pct,
      projected_usd: spent / elapsed,
      state: pct >= 100 ? 'exceeded' : pct >= b.warn_pct ? 'warning' : 'ok',
    };
  });
}

// ------------------------------------------------------------------- system

export function system(ds: Dataset, lang?: string): SystemInfo {
  const status = dataStatus(ds);
  const unpriced = status.unpriced_models;
  const database = { size_bytes: 18_400_000 + ds.records.length * 220, encrypted: true, key_source: 'keyring' as const, schema_version: 3 };
  const pricing = { version: '2026-11-01', source: 'built-in', models: Object.keys(PRICES_LIST).length, signed: false, unpriced_models: unpriced };
  const quarantine = quarantineRows(ds);
  const claudeCode = ds.scenario === 'empty' ? null : '2.4.1';
  return {
    version: '0.1.0-sample',
    data_dir: '/home/dev/.local/share/kanjo',
    claude_dir: '/home/dev/.claude',
    claude_code_version: claudeCode,
    database,
    ingest: ds.ingest,
    hooks: ds.hooks,
    pricing,
    server: { bind: '127.0.0.1', port: 7341, tls: false, auth: true },
    quarantine,
    retention: ds.settings.retention,
    checks: doctorChecks(
      ds,
      { sizeBytes: database.size_bytes, schema: database.schema_version, pricingVersion: pricing.version, unpriced, quarantined: quarantine.reduce((n, q) => n + q.count, 0), ccVersion: claudeCode },
      lang,
    ),
  };
}

const PRICES_LIST = { 'claude-opus-4-5': 1, 'claude-sonnet-4-5': 1, 'claude-haiku-4-5': 1, 'claude-fable-5-2': 1 };

export function auditPage(ds: Dataset, q: Q): AuditPage {
  const offset = q.offset ?? 0;
  return { items: ds.audit.slice(offset, offset + (q.limit ?? 50)), total: ds.audit.length, chain_ok: verifyChain(ds) };
}

// ------------------------------------------------------------------- search

export function search(ds: Dataset, q: string): SearchResult[] {
  const needle = q.toLowerCase().trim();
  if (!needle) return [];
  const out: SearchResult[] = [];
  for (const p of projects(ds)) {
    if (p.name.toLowerCase().includes(needle) || (p.client ?? '').toLowerCase().includes(needle))
      out.push({ kind: 'project', id: String(p.id), label: p.name, sublabel: p.client });
  }
  const models = new Set(ds.records.map((r) => r.model));
  for (const m of models) if (m.toLowerCase().includes(needle)) out.push({ kind: 'model', id: m, label: m, sublabel: m === UNPRICED_MODEL ? 'no price' : null });
  const grouped = bySession(ds);
  const sessions = [...ds.sessions].sort((a, b) => b.last - a.last);
  for (const s of sessions) {
    const sum = sessionSummary(ds, s, grouped.get(s.id) ?? []);
    const hay = [sum.title ?? '', sum.git_branch ?? '', sum.project, s.id].join(' ').toLowerCase();
    if (hay.includes(needle)) out.push({ kind: 'session', id: s.id, label: sum.title ?? s.id, sublabel: sum.git_branch ? `${sum.project}, ${sum.git_branch}` : sum.project });
    if (out.length >= 12) break;
  }
  return out.slice(0, 12);
}

export { between };
