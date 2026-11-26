import type {
  AlertList,
  AuditPage,
  BackupInfo,
  Block,
  Breakdown,
  Budget,
  BudgetInput,
  Comparison,
  Health,
  Heatmap,
  HookInstallRequest,
  HookStatus,
  IngestStatus,
  Insights,
  LimitsDetail,
  Overview,
  PlanFit,
  Project,
  ProjectPatch,
  PurgeReport,
  PurgeRequest,
  Query,
  SearchResult,
  SessionDetail,
  SessionList,
  Settings,
  SystemInfo,
  Transcript,
  Turn,
  UsageReport,
} from './contract';
import { errorFromResponse, networkError } from './errors';

export type FetchLike = (url: string, init: RequestInit) => Promise<Response>;
export type Method = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
export type QueryParams = Partial<{ [K in keyof Query]: Query[K] | undefined }>;
export type ExportView = 'usage' | 'breakdown' | 'sessions' | 'blocks' | 'turns' | 'alerts' | 'audit';
export type ExportFormat = 'csv' | 'json';

export interface ClientOptions {
  /** Absolute URL of the API root, ending in a slash (e.g. http://127.0.0.1:7341/api/v1/). */
  baseUrl: string;
  fetch: FetchLike;
  /** Called once per 401 response, before the error is thrown. */
  onUnauthorized?: () => void;
}

export interface Download {
  blob: Blob;
  filename: string;
}

/** The API root relative to the page, so the UI works behind any base path. */
export function defaultBaseUrl(doc: Pick<Document, 'baseURI'> = document): string {
  return new URL('api/v1/', doc.baseURI).toString();
}

export function buildUrl(baseUrl: string, path: string, query?: QueryParams): string {
  const url = new URL(path.replace(/^\//, ''), baseUrl);
  if (query) {
    for (const [k, v] of Object.entries(query)) {
      if (v === undefined || v === null || v === '') continue;
      url.searchParams.set(k, String(v));
    }
  }
  return url.toString();
}

const MUTATING: readonly Method[] = ['POST', 'PUT', 'PATCH', 'DELETE'];

export class ApiClient {
  private readonly opts: ClientOptions;

  constructor(opts: ClientOptions) {
    this.opts = opts;
  }

  get baseUrl(): string {
    return this.opts.baseUrl;
  }

  async request<T>(method: Method, path: string, init: { query?: QueryParams; body?: unknown } = {}): Promise<T> {
    const res = await this.raw(method, path, init);
    if (res.status === 204) return undefined as T;
    const text = await res.text();
    return (text ? JSON.parse(text) : undefined) as T;
  }

  private async raw(method: Method, path: string, init: { query?: QueryParams; body?: unknown }): Promise<Response> {
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (MUTATING.includes(method)) headers['X-Kanjo-CSRF'] = '1';
    if (init.body !== undefined) headers['Content-Type'] = 'application/json';
    let res: Response;
    try {
      res = await this.opts.fetch(buildUrl(this.opts.baseUrl, path, init.query), {
        method,
        headers,
        body: init.body === undefined ? undefined : JSON.stringify(init.body),
        credentials: 'same-origin',
        cache: 'no-store',
      });
    } catch (e) {
      throw networkError(e);
    }
    if (!res.ok) {
      let body: unknown;
      try {
        body = await res.json();
      } catch {
        body = undefined;
      }
      const err = errorFromResponse(res.status, body);
      if (err.code === 'unauthorized') this.opts.onUnauthorized?.();
      throw err;
    }
    return res;
  }

  health = () => this.request<Health>('GET', 'health');
  overview = () => this.request<Overview>('GET', 'overview');
  usage = (q: QueryParams = {}) => this.request<UsageReport>('GET', 'usage', { query: q });
  breakdown = (q: QueryParams = {}) => this.request<Breakdown>('GET', 'breakdown', { query: q });
  sessions = (q: QueryParams = {}) => this.request<SessionList>('GET', 'sessions', { query: q });
  session = (id: string) => this.request<SessionDetail>('GET', `sessions/${encodeURIComponent(id)}`);
  transcript = (id: string) => this.request<Transcript>('GET', `sessions/${encodeURIComponent(id)}/transcript`);
  turns = (q: QueryParams = {}) => this.request<Turn[]>('GET', 'turns', { query: q });
  projects = () => this.request<Project[]>('GET', 'projects');
  patchProject = (id: number, patch: ProjectPatch) => this.request<Project>('PATCH', `projects/${id}`, { body: patch });
  blocks = (q: QueryParams = {}) => this.request<Block[]>('GET', 'blocks', { query: q });
  limits = () => this.request<LimitsDetail>('GET', 'limits');
  heatmap = (q: QueryParams = {}) => this.request<Heatmap>('GET', 'heatmap', { query: q });
  compare = (period: 'week' | 'month', offset = 0) => this.request<Comparison>('GET', 'compare', { query: { period, offset } });
  plan = () => this.request<PlanFit>('GET', 'plan');
  insights = () => this.request<Insights>('GET', 'insights');
  alerts = (q: QueryParams = {}) => this.request<AlertList>('GET', 'alerts', { query: q });
  ackAlert = (id: number | null) => this.request<{ acknowledged: number }>('POST', 'alerts/ack', { body: { id } });
  budgets = () => this.request<Budget[]>('GET', 'budgets');
  createBudget = (b: BudgetInput) => this.request<Budget>('POST', 'budgets', { body: b });
  updateBudget = (id: number, b: BudgetInput) => this.request<Budget>('PUT', `budgets/${id}`, { body: b });
  deleteBudget = (id: number) => this.request<void>('DELETE', `budgets/${id}`);
  settings = () => this.request<Settings>('GET', 'settings');
  saveSettings = (s: Settings) => this.request<Settings>('PUT', 'settings', { body: s });
  system = (q: QueryParams = {}) => this.request<SystemInfo>('GET', 'system', { query: q });
  installHooks = (r: HookInstallRequest) => this.request<HookStatus>('POST', 'system/hooks', { body: r });
  removeHooks = () => this.request<HookStatus>('DELETE', 'system/hooks');
  backups = () => this.request<BackupInfo[]>('GET', 'system/backups');
  createBackup = () => this.request<BackupInfo>('POST', 'system/backup');
  purge = (r: PurgeRequest) => this.request<PurgeReport>('POST', 'system/purge', { body: r });
  rescan = () => this.request<IngestStatus>('POST', 'system/rescan');
  audit = (q: QueryParams = {}) => this.request<AuditPage>('GET', 'audit', { query: q });
  search = (q: string) => this.request<SearchResult[]>('GET', 'search', { query: { q } });

  /** Downloads an export through the same auth path as every other call. */
  async exportView(view: ExportView, format: ExportFormat, q: QueryParams = {}): Promise<Download> {
    const res = await this.raw('GET', `export/${view}`, { query: { ...q, format } });
    const cd = res.headers.get('Content-Disposition') ?? '';
    const match = /filename="?([^";]+)"?/i.exec(cd);
    return { blob: await res.blob(), filename: match?.[1] ?? `kanjo-${view}.${format}` };
  }

  eventsUrl(): string {
    return buildUrl(this.opts.baseUrl, 'events');
  }
}
