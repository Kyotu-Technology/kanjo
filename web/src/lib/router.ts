export type RouteName =
  | 'dashboard'
  | 'usage'
  | 'sessions'
  | 'session'
  | 'limits'
  | 'insights'
  | 'alerts'
  | 'projects'
  | 'settings'
  | 'system'
  | 'not_found';

export interface Route {
  name: RouteName;
  path: string;
  params: Record<string, string>;
  query: Record<string, string>;
}

const PATTERNS: [RouteName, string][] = [
  ['dashboard', '/'],
  ['usage', '/usage'],
  ['sessions', '/sessions'],
  ['session', '/sessions/:id'],
  ['limits', '/limits'],
  ['insights', '/insights'],
  ['alerts', '/alerts'],
  ['projects', '/projects'],
  ['settings', '/settings'],
  ['system', '/system'],
];

/** Parses a location hash such as `#/sessions/abc?tab=turns`. */
export function matchRoute(hash: string): Route {
  const raw = hash.replace(/^#/, '') || '/';
  const [pathPart, queryPart = ''] = raw.split('?');
  const path = '/' + pathPart.replace(/^\/+|\/+$/g, '');
  const query = Object.fromEntries(new URLSearchParams(queryPart));
  const parts = path.split('/').filter(Boolean);
  for (const [name, pattern] of PATTERNS) {
    const pp = pattern.split('/').filter(Boolean);
    if (pp.length !== parts.length) continue;
    const params: Record<string, string> = {};
    let ok = true;
    for (let i = 0; i < pp.length; i++) {
      if (pp[i].startsWith(':')) params[pp[i].slice(1)] = decodeURIComponent(parts[i]);
      else if (pp[i] !== parts[i]) ok = false;
    }
    if (ok) return { name, path, params, query };
  }
  return { name: 'not_found', path, params: {}, query };
}

/** Builds a hash link; works under any base path because only the fragment changes. */
export function href(path: string, query: Record<string, string | number | undefined | null> = {}): string {
  const qs = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) if (v !== undefined && v !== null && v !== '') qs.set(k, String(v));
  const s = qs.toString();
  return `#${path}${s ? `?${s}` : ''}`;
}
