import { asApiError, type ApiRequestError } from './errors';

export type QueryStatus = 'idle' | 'loading' | 'success' | 'error';

/** Immutable view of one cached query; a new object is emitted on every change. */
export interface Snapshot<T> {
  readonly key: string;
  readonly data: T | undefined;
  readonly error: ApiRequestError | undefined;
  readonly status: QueryStatus;
  /** A request is in flight (first load or background revalidation). */
  readonly fetching: boolean;
  readonly updatedAt: number;
}

export interface QueryOptions<T> {
  /** Unique per endpoint and parameters, e.g. `usage?from=1&group=day`. */
  key: string;
  fetch: () => Promise<T>;
  /** Invalidation tags; live events invalidate by tag (`usage`, `limits`, `alerts`, ...). */
  tags?: readonly string[];
  /** Data older than this is shown and revalidated in the background. */
  staleMs?: number;
}

export interface CacheOptions {
  now?: () => number;
  staleMs?: number;
  /** Unobserved entries older than this are dropped. */
  gcMs?: number;
}

type Listener = (s: Snapshot<unknown>) => void;

interface Entry {
  snapshot: Snapshot<unknown>;
  fetcher: () => Promise<unknown>;
  tags: readonly string[];
  staleMs: number;
  listeners: Set<Listener>;
  inflight: Promise<unknown> | undefined;
  invalidated: boolean;
  lastUsed: number;
}

/** Small stale-while-revalidate cache keyed by string, framework independent. */
export class QueryCache {
  private readonly entries = new Map<string, Entry>();
  private readonly now: () => number;
  private readonly staleMs: number;
  private readonly gcMs: number;

  constructor(opts: CacheOptions = {}) {
    this.now = opts.now ?? Date.now;
    this.staleMs = opts.staleMs ?? 30_000;
    this.gcMs = opts.gcMs ?? 5 * 60_000;
  }

  private entry<T>(opts: QueryOptions<T>): Entry {
    let e = this.entries.get(opts.key);
    if (!e) {
      e = {
        snapshot: { key: opts.key, data: undefined, error: undefined, status: 'idle', fetching: false, updatedAt: 0 },
        fetcher: opts.fetch,
        tags: opts.tags ?? [],
        staleMs: opts.staleMs ?? this.staleMs,
        listeners: new Set(),
        inflight: undefined,
        invalidated: false,
        lastUsed: this.now(),
      };
      this.entries.set(opts.key, e);
    } else {
      e.fetcher = opts.fetch;
      e.tags = opts.tags ?? e.tags;
      e.staleMs = opts.staleMs ?? e.staleMs;
    }
    return e;
  }

  private set(e: Entry, patch: Partial<Snapshot<unknown>>): void {
    e.snapshot = { ...e.snapshot, ...patch };
    for (const l of e.listeners) l(e.snapshot);
  }

  private isStale(e: Entry): boolean {
    const s = e.snapshot;
    return e.invalidated || s.status === 'idle' || s.status === 'error' || this.now() - s.updatedAt >= e.staleMs;
  }

  private run(e: Entry): Promise<unknown> {
    if (e.inflight) return e.inflight;
    e.invalidated = false;
    this.set(e, { fetching: true, status: e.snapshot.data === undefined ? 'loading' : e.snapshot.status });
    const p = e
      .fetcher()
      .then(
        (data) => {
          this.set(e, { data, error: undefined, status: 'success', fetching: false, updatedAt: this.now() });
          return data;
        },
        (err: unknown) => {
          this.set(e, { error: asApiError(err), status: 'error', fetching: false, updatedAt: this.now() });
          throw err;
        },
      )
      .finally(() => {
        e.inflight = undefined;
      });
    e.inflight = p;
    return p;
  }

  /**
   * Subscribes to a query. The listener gets the current snapshot at once and
   * every change after; a stale or missing value is (re)fetched.
   */
  observe<T>(opts: QueryOptions<T>, listener: (s: Snapshot<T>) => void): () => void {
    const e = this.entry(opts);
    const l = listener as Listener;
    e.listeners.add(l);
    e.lastUsed = this.now();
    l(e.snapshot);
    if (this.isStale(e)) this.run(e).catch(() => undefined);
    return () => {
      e.listeners.delete(l);
      e.lastUsed = this.now();
      this.gc();
    };
  }

  /** Imperative read: cached data when fresh, otherwise a (deduplicated) request. */
  async fetch<T>(opts: QueryOptions<T>): Promise<T> {
    const e = this.entry(opts);
    if (!this.isStale(e) && e.snapshot.data !== undefined) return e.snapshot.data as T;
    return (await this.run(e)) as T;
  }

  async refetch(key: string): Promise<void> {
    const e = this.entries.get(key);
    if (e) await this.run(e).catch(() => undefined);
  }

  /**
   * Marks matching entries stale. Observed ones refetch now and keep showing
   * their old data meanwhile; the rest refetch when next observed.
   * Returns the number of entries refetched.
   */
  invalidate(match: readonly string[] | ((key: string, tags: readonly string[]) => boolean)): number {
    const test = typeof match === 'function' ? match : (_k: string, tags: readonly string[]) => tags.some((t) => match.includes(t));
    let n = 0;
    for (const [key, e] of this.entries) {
      if (!test(key, e.tags)) continue;
      e.invalidated = true;
      if (e.listeners.size > 0) {
        n++;
        this.run(e).catch(() => undefined);
      }
    }
    return n;
  }

  /** Replaces cached data locally (optimistic updates, live patches). */
  update<T>(key: string, fn: (old: T) => T): boolean {
    const e = this.entries.get(key);
    if (!e || e.snapshot.data === undefined) return false;
    this.set(e, { data: fn(e.snapshot.data as T) });
    return true;
  }

  peek<T>(key: string): Snapshot<T> | undefined {
    return this.entries.get(key)?.snapshot as Snapshot<T> | undefined;
  }

  observers(key: string): number {
    return this.entries.get(key)?.listeners.size ?? 0;
  }

  clear(): void {
    this.entries.clear();
  }

  private gc(): void {
    const limit = this.now() - this.gcMs;
    for (const [key, e] of this.entries) {
      if (e.listeners.size === 0 && !e.inflight && e.lastUsed < limit) this.entries.delete(key);
    }
  }
}

/** Builds a stable cache key from an endpoint name and its parameters. */
export function queryKey(name: string, params: Record<string, unknown> = {}): string {
  const parts = Object.keys(params)
    .filter((k) => params[k] !== undefined && params[k] !== null && params[k] !== '')
    .sort()
    .map((k) => `${k}=${String(params[k])}`);
  return parts.length ? `${name}?${parts.join('&')}` : name;
}
