import type { QueryOptions, Snapshot } from './cache';
import type { ApiRequestError } from './errors';
import { cache } from '../stores/app.svelte';

export interface QueryResult<T> {
  /** Current data, or the previous key's data while a new key loads. */
  readonly data: T | undefined;
  readonly error: ApiRequestError | undefined;
  /** Nothing to show yet: render a skeleton. */
  readonly loading: boolean;
  /** Showing data while a newer copy is fetched: dim, do not blank. */
  readonly refreshing: boolean;
  refetch(): Promise<void>;
}

/**
 * Subscribes a component to a cached query. `options` is re-read reactively,
 * so a key built from component state refetches when that state changes.
 */
export function useQuery<T>(options: () => QueryOptions<T>): QueryResult<T> {
  let snap = $state.raw<Snapshot<T> | undefined>(undefined);
  let previous = $state.raw<T | undefined>(undefined);
  let key = '';

  $effect(() => {
    const o = options();
    key = o.key;
    return cache.observe(o, (s) => {
      snap = s;
      if (s.data !== undefined) previous = s.data;
    });
  });

  return {
    get data() {
      return snap?.data ?? previous;
    },
    get error() {
      return snap?.error;
    },
    get loading() {
      return (snap?.data ?? previous) === undefined && snap?.status !== 'error';
    },
    get refreshing() {
      return (snap?.fetching ?? false) && (snap?.data ?? previous) !== undefined;
    },
    refetch: () => cache.refetch(key),
  };
}
