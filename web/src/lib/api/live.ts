import type { LiveEvent } from './contract';

export type LiveStatus = 'connecting' | 'open' | 'reconnecting' | 'stopped';

/** The part of EventSource the connection uses; the mock provides its own. */
export interface EventSourceLike {
  onopen: ((ev: Event) => void) | null;
  onmessage: ((ev: MessageEvent<string>) => void) | null;
  onerror: ((ev: Event) => void) | null;
  close(): void;
}

export interface LiveOptions {
  url: string;
  create: (url: string) => EventSourceLike;
  onEvent: (e: LiveEvent) => void;
  onStatus?: (s: LiveStatus, attempt: number) => void;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (id: unknown) => void;
  random?: () => number;
  baseDelayMs?: number;
  maxDelayMs?: number;
}

/** Exponential backoff with +-20% jitter: 1 s, 2 s, 4 s ... capped at maxMs. */
export function backoffDelay(attempt: number, random: () => number = Math.random, baseMs = 1000, maxMs = 30_000): number {
  const raw = Math.min(maxMs, baseMs * 2 ** Math.max(0, attempt));
  const jitter = 0.8 + random() * 0.4;
  return Math.round(Math.min(maxMs, raw * jitter));
}

export function parseLiveEvent(data: string): LiveEvent | undefined {
  try {
    const v = JSON.parse(data) as { type?: unknown };
    if (typeof v !== 'object' || v === null || typeof v.type !== 'string') return undefined;
    if (!['usage', 'limits', 'alert', 'ingest', 'status'].includes(v.type)) return undefined;
    return v as LiveEvent;
  } catch {
    return undefined;
  }
}

/** Which cache tags an event makes stale. */
export function tagsFor(e: LiveEvent): string[] {
  switch (e.type) {
    case 'usage':
      return ['usage', 'sessions', 'budgets'];
    case 'limits':
      return ['limits'];
    case 'alert':
      return ['alerts'];
    case 'ingest':
      return e.data.backfill_running ? ['ingest'] : ['ingest', 'system', 'usage', 'sessions'];
    case 'status':
      return ['sessions', `session:${e.data.session_id}`];
  }
}

/** One SSE connection that reconnects with backoff until stopped. */
export class LiveConnection {
  private readonly o: Required<Omit<LiveOptions, 'onStatus'>> & Pick<LiveOptions, 'onStatus'>;
  private es: EventSourceLike | undefined;
  private timer: unknown;
  private attempt = 0;
  private stopped = true;
  status: LiveStatus = 'stopped';

  constructor(opts: LiveOptions) {
    this.o = {
      setTimer: (fn, ms) => setTimeout(fn, ms),
      clearTimer: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
      random: Math.random,
      baseDelayMs: 1000,
      maxDelayMs: 30_000,
      ...opts,
    };
  }

  start(): void {
    if (!this.stopped) return;
    this.stopped = false;
    this.attempt = 0;
    this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.timer !== undefined) this.o.clearTimer(this.timer);
    this.timer = undefined;
    this.es?.close();
    this.es = undefined;
    this.setStatus('stopped');
  }

  /** Reconnects now (e.g. when the tab becomes visible again) instead of waiting for the timer. */
  retryNow(): void {
    if (this.stopped || this.status === 'open' || this.status === 'connecting') return;
    if (this.timer !== undefined) this.o.clearTimer(this.timer);
    this.timer = undefined;
    this.connect();
  }

  private setStatus(s: LiveStatus): void {
    this.status = s;
    this.o.onStatus?.(s, this.attempt);
  }

  private connect(): void {
    this.setStatus(this.attempt === 0 ? 'connecting' : 'reconnecting');
    const es = this.o.create(this.o.url);
    this.es = es;
    es.onopen = () => {
      if (this.es !== es) return;
      this.attempt = 0;
      this.setStatus('open');
    };
    es.onmessage = (ev) => {
      if (this.es !== es) return;
      const parsed = parseLiveEvent(ev.data);
      if (parsed) this.o.onEvent(parsed);
    };
    es.onerror = () => {
      if (this.es !== es) return;
      es.close();
      this.es = undefined;
      if (this.stopped) return;
      const delay = backoffDelay(this.attempt, this.o.random, this.o.baseDelayMs, this.o.maxDelayMs);
      this.attempt++;
      this.setStatus('reconnecting');
      this.timer = this.o.setTimer(() => {
        this.timer = undefined;
        if (!this.stopped) this.connect();
      }, delay);
    };
  }
}

/** Collects tags for a short window so a burst of events causes one refetch. */
export class TagBatcher {
  private pending = new Set<string>();
  private timer: unknown;

  constructor(
    private readonly flush: (tags: string[]) => void,
    private readonly delayMs = 400,
    private readonly setTimer: (fn: () => void, ms: number) => unknown = (fn, ms) => setTimeout(fn, ms),
  ) {}

  add(tags: readonly string[]): void {
    for (const t of tags) this.pending.add(t);
    if (this.timer !== undefined) return;
    this.timer = this.setTimer(() => {
      this.timer = undefined;
      const tags = [...this.pending];
      this.pending.clear();
      if (tags.length) this.flush(tags);
    }, this.delayMs);
  }
}
