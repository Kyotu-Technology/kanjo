import type { QueryOptions } from './cache';
import { queryKey } from './cache';
import type { QueryParams } from './client';
import type {
  AlertList,
  AuditPage,
  BackupInfo,
  Block,
  Breakdown,
  Budget,
  Comparison,
  Heatmap,
  Insights,
  LimitsDetail,
  Overview,
  PlanFit,
  Project,
  SessionDetail,
  SessionList,
  SystemInfo,
  Turn,
  UsageReport,
} from './contract';
import { api } from '../stores/app.svelte';

/**
 * One builder per read endpoint. Tags tie each query to the live events that
 * make it stale: `usage` (new requests), `limits`, `alerts`, `sessions`,
 * `ingest`, `system`, `budgets`, `session:<id>`.
 */
export const queries = {
  overview: (): QueryOptions<Overview> => ({ key: 'overview', fetch: () => api().overview(), tags: ['usage', 'limits', 'sessions', 'ingest'] }),
  usage: (p: QueryParams): QueryOptions<UsageReport> => ({ key: queryKey('usage', p), fetch: () => api().usage(p), tags: ['usage'] }),
  breakdown: (p: QueryParams): QueryOptions<Breakdown> => ({ key: queryKey('breakdown', p), fetch: () => api().breakdown(p), tags: ['usage'] }),
  sessions: (p: QueryParams): QueryOptions<SessionList> => ({ key: queryKey('sessions', p), fetch: () => api().sessions(p), tags: ['usage', 'sessions'] }),
  session: (id: string): QueryOptions<SessionDetail> => ({ key: `session/${id}`, fetch: () => api().session(id), tags: ['usage', `session:${id}`] }),
  turns: (p: QueryParams): QueryOptions<Turn[]> => ({ key: queryKey('turns', p), fetch: () => api().turns(p), tags: ['usage'] }),
  projects: (): QueryOptions<Project[]> => ({ key: 'projects', fetch: () => api().projects(), tags: ['usage', 'projects'] }),
  blocks: (p: QueryParams): QueryOptions<Block[]> => ({ key: queryKey('blocks', p), fetch: () => api().blocks(p), tags: ['usage', 'limits'] }),
  limits: (): QueryOptions<LimitsDetail> => ({ key: 'limits', fetch: () => api().limits(), tags: ['limits', 'usage'] }),
  heatmap: (p: QueryParams): QueryOptions<Heatmap> => ({ key: queryKey('heatmap', p), fetch: () => api().heatmap(p), tags: ['usage'], staleMs: 120_000 }),
  compare: (period: 'week' | 'month', offset = 0): QueryOptions<Comparison> => ({
    key: queryKey('compare', { period, offset }),
    fetch: () => api().compare(period, offset),
    tags: ['usage'],
  }),
  plan: (): QueryOptions<PlanFit> => ({ key: 'plan', fetch: () => api().plan(), tags: ['usage', 'settings'], staleMs: 300_000 }),
  insights: (): QueryOptions<Insights> => ({ key: 'insights', fetch: () => api().insights(), tags: ['usage'], staleMs: 120_000 }),
  alerts: (p: QueryParams): QueryOptions<AlertList> => ({ key: queryKey('alerts', p), fetch: () => api().alerts(p), tags: ['alerts'] }),
  budgets: (): QueryOptions<Budget[]> => ({ key: 'budgets', fetch: () => api().budgets(), tags: ['budgets', 'usage'] }),
  system: (lang?: string): QueryOptions<SystemInfo> => ({
    key: lang ? queryKey('system', { lang }) : 'system',
    fetch: () => api().system(lang ? { lang } : {}),
    tags: ['system', 'ingest'],
  }),
  backups: (): QueryOptions<BackupInfo[]> => ({ key: 'backups', fetch: () => api().backups(), tags: ['system'] }),
  audit: (p: QueryParams): QueryOptions<AuditPage> => ({ key: queryKey('audit', p), fetch: () => api().audit(p), tags: ['system'] }),
};
