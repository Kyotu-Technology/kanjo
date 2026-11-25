import { describe, expect, test, vi } from 'vitest';
import type { LiveEvent } from './contract';
import { backoffDelay, LiveConnection, parseLiveEvent, TagBatcher, tagsFor, type EventSourceLike, type LiveStatus } from './live';

class FakeSource implements EventSourceLike {
  onopen: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent<string>) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  closed = false;
  close() {
    this.closed = true;
  }
}

describe('backoff', () => {
  test('doubles and caps, with jitter inside +-20%', () => {
    const mid = () => 0.5;
    expect(backoffDelay(0, mid)).toBe(1000);
    expect(backoffDelay(3, mid)).toBe(8000);
    expect(backoffDelay(20, mid)).toBe(30000);
    expect(backoffDelay(2, () => 0)).toBe(3200);
    expect(backoffDelay(2, () => 1)).toBe(4800);
  });
});

describe('LiveConnection', () => {
  test('reconnects with growing delays and resets after a successful open', () => {
    vi.useFakeTimers();
    const sources: FakeSource[] = [];
    const statuses: LiveStatus[] = [];
    const events: LiveEvent[] = [];
    const conn = new LiveConnection({
      url: 'http://x/api/v1/events',
      create: () => {
        const s = new FakeSource();
        sources.push(s);
        return s;
      },
      onEvent: (e) => events.push(e),
      onStatus: (s) => statuses.push(s),
      random: () => 0.5,
    });
    conn.start();
    expect(sources).toHaveLength(1);
    sources[0].onerror?.(new Event('error'));
    expect(sources[0].closed).toBe(true);
    expect(conn.status).toBe('reconnecting');
    vi.advanceTimersByTime(999);
    expect(sources).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sources).toHaveLength(2);
    sources[1].onerror?.(new Event('error'));
    vi.advanceTimersByTime(1999);
    expect(sources).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(sources).toHaveLength(3);
    sources[2].onopen?.(new Event('open'));
    expect(conn.status).toBe('open');
    sources[2].onmessage?.(new MessageEvent('message', { data: JSON.stringify({ type: 'status', data: { session_id: 's1' } }) }));
    sources[2].onmessage?.(new MessageEvent('message', { data: 'not json' }));
    expect(events).toEqual([{ type: 'status', data: { session_id: 's1' } }]);
    sources[2].onerror?.(new Event('error'));
    vi.advanceTimersByTime(1000);
    expect(sources).toHaveLength(4);
    conn.stop();
    expect(sources[3].closed).toBe(true);
    expect(statuses.at(-1)).toBe('stopped');
    vi.useRealTimers();
  });

  test('retryNow skips the wait', () => {
    vi.useFakeTimers();
    const sources: FakeSource[] = [];
    const conn = new LiveConnection({
      url: 'u',
      create: () => {
        const s = new FakeSource();
        sources.push(s);
        return s;
      },
      onEvent: () => undefined,
      random: () => 0.5,
    });
    conn.start();
    sources[0].onerror?.(new Event('error'));
    conn.retryNow();
    expect(sources).toHaveLength(2);
    conn.stop();
    vi.useRealTimers();
  });
});

describe('events', () => {
  test('parse rejects unknown types', () => {
    expect(parseLiveEvent('{"type":"nope","data":1}')).toBeUndefined();
    expect(parseLiveEvent('{"type":"alert","data":{}}')?.type).toBe('alert');
  });

  test('tags follow the event', () => {
    expect(tagsFor({ type: 'usage', data: { requests: 1, cost_usd: 0.1, sessions: [] } })).toContain('usage');
    expect(tagsFor({ type: 'status', data: { session_id: 'a' } })).toContain('session:a');
    const ingest = { backfill_running: false, watcher_running: true, files_total: 1, files_done: 1, bytes_total: 1, bytes_done: 1, requests_stored: 1, last_event_at: null };
    expect(tagsFor({ type: 'ingest', data: ingest })).toContain('usage');
    expect(tagsFor({ type: 'ingest', data: { ...ingest, backfill_running: true } })).not.toContain('usage');
  });

  test('batcher coalesces a burst into one flush', () => {
    vi.useFakeTimers();
    const flush = vi.fn();
    const b = new TagBatcher(flush, 100);
    b.add(['usage']);
    b.add(['limits', 'usage']);
    vi.advanceTimersByTime(100);
    expect(flush).toHaveBeenCalledTimes(1);
    expect(flush.mock.calls[0][0].sort()).toEqual(['limits', 'usage']);
    vi.useRealTimers();
  });
});
