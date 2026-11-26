import { ApiClient, defaultBaseUrl, type FetchLike } from '../api/client';
import { QueryCache } from '../api/cache';
import type { Alert, IngestStatus, Limits, Overview, Settings, Theme } from '../api/contract';
import { asApiError } from '../api/errors';
import { LiveConnection, TagBatcher, tagsFor, watchAuth, type EventSourceLike, type LiveStatus } from '../api/live';
import { i18n, resolveLang, setLang, t, type Lang } from '../i18n/index.svelte';
import { theme } from './theme.svelte';
import { toasts } from './toast.svelte';

export type AuthState = 'ok' | 'unauthorized';
export type BootState = 'loading' | 'ready' | 'failed';

class AppState {
  boot = $state<BootState>('loading');
  bootError = $state<string | null>(null);
  auth = $state<AuthState>('ok');
  live = $state<LiveStatus>('stopped');
  ingest = $state<IngestStatus | null>(null);
  unread = $state(0);
  /** Name of the mock scenario when sample data is shown, otherwise null. */
  mock = $state<string | null>(null);
  settings = $state.raw<Settings | null>(null);
  paletteOpen = $state(false);
}

export const app = new AppState();
export const cache = new QueryCache({ staleMs: 30_000 });

let client: ApiClient | undefined;
let createEventSource: (url: string) => EventSourceLike = (url) => new EventSource(url, { withCredentials: true }) as unknown as EventSourceLike;
let live: LiveConnection | undefined;

/** The API client; available once bootstrap() has run. */
export function api(): ApiClient {
  if (!client) throw new Error('Kanjo API used before bootstrap');
  return client;
}

/** `?mock` or `?mock=<scenario>` in the URL, or VITE_MOCK=1 at build time. */
export function mockScenario(search: string, env: string | undefined): string | null {
  const p = new URLSearchParams(search);
  if (p.has('mock')) return p.get('mock') || 'default';
  if (env === '1' || env === 'true') return 'default';
  return null;
}

function urlOverride(name: 'lang' | 'theme'): string | null {
  return new URLSearchParams(location.search).get(name);
}

export function applySettings(s: Settings, opts: { fromUrl?: boolean } = {}): void {
  app.settings = s;
  const langParam = opts.fromUrl ? urlOverride('lang') : null;
  setLang(resolveLang(langParam === 'pl' || langParam === 'en' ? langParam : s.language));
  i18n.currency = s.currency;
  const themeParam = opts.fromUrl ? urlOverride('theme') : null;
  theme.apply(themeParam === 'light' || themeParam === 'dark' ? (themeParam as Theme) : s.theme);
}

/** Optimistically applies a settings change, then persists it; reverts on failure. */
export async function updateSettings(change: (s: Settings) => Settings): Promise<boolean> {
  const before = app.settings;
  if (!before) return false;
  const next = change(structuredClone(before));
  applySettings(next);
  try {
    applySettings(await api().saveSettings(next));
    return true;
  } catch (e) {
    applySettings(before);
    toasts.push('error', describeError(e));
    return false;
  }
}

export function setLanguage(lang: Lang): Promise<boolean> {
  return updateSettings((s) => ({ ...s, language: lang }));
}

export function setTheme(pref: Theme): Promise<boolean> {
  return updateSettings((s) => ({ ...s, theme: pref }));
}

export function describeError(e: unknown): string {
  const err = asApiError(e);
  switch (err.code) {
    case 'network':
      return t('common.error_network');
    case 'disabled':
      return t('common.error_disabled');
    case 'not_found':
      return t('common.error_not_found');
    case 'invalid':
      return t('common.error_invalid', { message: err.message });
    default:
      return t('common.error_generic', { message: err.message });
  }
}

function onLiveEvent(batcher: TagBatcher, e: Parameters<typeof tagsFor>[0]): void {
  switch (e.type) {
    case 'limits':
      cache.update<Overview>('overview', (o) => ({ ...o, limits: e.data as Limits }));
      break;
    case 'alert': {
      const a = e.data as Alert;
      app.unread++;
      toasts.push(a.severity === 'critical' ? 'error' : a.severity === 'warning' ? 'warning' : 'info', a.title);
      break;
    }
    case 'ingest':
      app.ingest = e.data as IngestStatus;
      break;
  }
  batcher.add(tagsFor(e));
}

const authWatch = watchAuth(
  () => api().settings(),
  () => app.auth === 'unauthorized',
  () => live?.stop(),
);

function startLive(): void {
  const batcher = new TagBatcher((tags) => cache.invalidate(tags), 400);
  live = new LiveConnection({
    url: api().eventsUrl(),
    create: createEventSource,
    onEvent: (e) => onLiveEvent(batcher, e),
    onStatus: (s, attempt) => {
      app.live = s;
      authWatch(s, attempt);
    },
  });
  live.start();
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') live?.retryNow();
  });
}

export async function refreshUnread(): Promise<void> {
  try {
    const list = await api().alerts({ q: 'open', limit: 1, lang: i18n.lang });
    app.unread = list.unacknowledged;
  } catch {
    // the bell keeps its last count; the alerts page shows the error
  }
}

export async function bootstrap(): Promise<void> {
  const scenario = mockScenario(location.search, import.meta.env.VITE_MOCK as string | undefined);
  let fetchImpl: FetchLike = (url, init) => fetch(url, init);
  if (scenario !== null) {
    const mock = await import('../mock/index');
    const server = mock.createMock({ scenario: mock.scenarioFrom(scenario) });
    fetchImpl = server.fetch;
    createEventSource = server.eventSource;
    app.mock = mock.scenarioFrom(scenario);
  }
  client = new ApiClient({
    baseUrl: defaultBaseUrl(),
    fetch: fetchImpl,
    onUnauthorized: () => {
      app.auth = 'unauthorized';
    },
  });
  setInterval(() => (i18n.now = Date.now()), 30_000);
  try {
    applySettings(await client.settings(), { fromUrl: true });
    app.boot = 'ready';
  } catch (e) {
    if (app.auth === 'unauthorized') {
      applyFallbackPrefs();
      app.boot = 'ready';
      return;
    }
    applyFallbackPrefs();
    app.bootError = describeError(e);
    app.boot = 'failed';
    return;
  }
  void refreshUnread();
  client
    .system()
    .then((s) => (app.ingest = s.ingest))
    .catch(() => undefined);
  startLive();
}

function applyFallbackPrefs(): void {
  const l = urlOverride('lang');
  setLang(resolveLang(l));
  const th = urlOverride('theme');
  theme.apply(th === 'light' || th === 'dark' ? th : 'system');
}

/** Retries after the user ran `kanjo open` (the cookie may now be set). */
export async function retryAuth(): Promise<void> {
  app.auth = 'ok';
  cache.clear();
  app.boot = 'loading';
  try {
    applySettings(await api().settings(), { fromUrl: true });
    app.boot = 'ready';
    if (!live) startLive();
    else live.start();
    void refreshUnread();
  } catch (e) {
    if ((app.auth as AuthState) !== 'unauthorized') {
      app.bootError = describeError(e);
      app.boot = 'failed';
    } else app.boot = 'ready';
  }
}
