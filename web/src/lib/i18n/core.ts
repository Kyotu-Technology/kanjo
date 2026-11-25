export interface Plural {
  readonly $plural: true;
  readonly one: string;
  readonly few?: string;
  readonly many?: string;
  readonly other: string;
}

/** Plural forms chosen by Intl.PluralRules; Polish needs one, few, many and other. */
export function plural(forms: { one: string; few?: string; many?: string; other: string }): Plural {
  return { $plural: true, ...forms };
}

export type Entry = string | Plural;
export interface Dict {
  readonly [key: string]: Entry | Dict;
}

/** Dotted paths to every leaf of a dictionary, e.g. `dashboard.limits.title`. */
export type Keys<T> = {
  [K in keyof T & string]: T[K] extends string | Plural ? K : `${K}.${Keys<T[K]>}`;
}[keyof T & string];

export type Params = Record<string, string | number>;

export function isPlural(v: unknown): v is Plural {
  return typeof v === 'object' && v !== null && (v as Plural).$plural === true;
}

export function lookup(dict: Dict, key: string): Entry | undefined {
  let node: Entry | Dict | undefined = dict;
  for (const part of key.split('.')) {
    if (typeof node !== 'object' || node === null || isPlural(node)) return undefined;
    node = (node as Dict)[part];
  }
  return typeof node === 'string' || isPlural(node) ? node : undefined;
}

const rulesCache = new Map<string, Intl.PluralRules>();

function pluralRules(locale: string): Intl.PluralRules {
  let r = rulesCache.get(locale);
  if (!r) {
    r = new Intl.PluralRules(locale);
    rulesCache.set(locale, r);
  }
  return r;
}

export function interpolate(text: string, params?: Params): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) => (name in params ? String(params[name]) : m));
}

/**
 * Resolves a key. Plural entries select their form from `params.count`; the
 * `{count}` placeholder may be pre-formatted by passing `countLabel`.
 */
export function translate(dict: Dict, locale: string, key: string, params?: Params, fallback?: Dict): string {
  const entry = lookup(dict, key) ?? (fallback ? lookup(fallback, key) : undefined);
  if (entry === undefined) return key;
  if (typeof entry === 'string') return interpolate(entry, params);
  const count = Number(params?.count ?? 0);
  const form = pluralRules(locale).select(count) as 'zero' | 'one' | 'two' | 'few' | 'many' | 'other';
  const text = (form === 'one' || form === 'few' || form === 'many' ? entry[form] : undefined) ?? entry.other;
  const shown = params?.countLabel !== undefined ? { ...params, count: params.countLabel } : params;
  return interpolate(text, shown);
}

/** Every leaf as [path, entry]; used by the completeness test. */
export function flatten(dict: Dict, prefix = ''): [string, Entry][] {
  const out: [string, Entry][] = [];
  for (const [k, v] of Object.entries(dict)) {
    const path = prefix ? `${prefix}.${k}` : k;
    if (typeof v === 'string' || isPlural(v)) out.push([path, v]);
    else out.push(...flatten(v as Dict, path));
  }
  return out;
}

export function placeholders(text: string): string[] {
  return [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
}
