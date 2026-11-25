import type { CurrencySettings, Language } from '../api/contract';
import { translate, type Keys, type Params } from './core';
import en, { type Messages } from './en';
import pl from './pl';
import {
  formatAgo,
  formatCompact,
  formatDate,
  formatDateTime,
  formatDuration,
  formatMoney,
  formatNumber,
  formatPct,
  formatRatio,
  formatTime,
  formatWhen,
  intlLocale,
  type Lang,
  type MoneyOptions,
} from './format';

export type { Lang } from './format';
export type MessageKey = Keys<Messages>;

const dicts: Record<Lang, Messages> = { en, pl };

export function resolveLang(setting: Language | string | null | undefined, browser = typeof navigator === 'undefined' ? 'en' : navigator.language): Lang {
  if (setting === 'pl' || setting === 'en') return setting;
  return browser.toLowerCase().startsWith('pl') ? 'pl' : 'en';
}

class Prefs {
  lang = $state<Lang>('en');
  currency = $state<CurrencySettings>({ code: 'USD', usd_rate: 1 });
  /** Ticks once a minute so relative times and countdowns stay current. */
  now = $state(Date.now());

  get locale(): string {
    return intlLocale(this.lang);
  }
}

export const i18n = new Prefs();

export function setLang(lang: Lang): void {
  i18n.lang = lang;
  if (typeof document !== 'undefined') document.documentElement.lang = lang;
}

export function t(key: MessageKey, params?: Params): string {
  return translate(dicts[i18n.lang], i18n.locale, key, params, en);
}

/** Untyped lookup for keys built at runtime (`plan.${kind}`); falls back to English, then to the key. */
export function tk(key: string, params?: Params): string {
  return translate(dicts[i18n.lang], i18n.locale, key, params, en);
}

/** Plural helper: formats the count in the current locale before inserting it. */
export function tn(key: MessageKey, count: number, params?: Params): string {
  return translate(dicts[i18n.lang], i18n.locale, key, { ...params, count, countLabel: formatNumber(count, i18n.locale) }, en);
}

export const fmt = {
  money: (usd: number, o?: MoneyOptions) => formatMoney(usd, i18n.currency, i18n.locale, o),
  number: (n: number, digits = 0) => formatNumber(n, i18n.locale, digits),
  compact: (n: number) => formatCompact(n, i18n.locale),
  pct: (pct: number, digits = 0) => formatPct(pct, i18n.locale, digits),
  ratio: (r: number, digits = 0) => formatRatio(r, i18n.locale, digits),
  time: (ts: number) => formatTime(ts, i18n.locale),
  date: (ts: number, style: 'short' | 'medium' | 'long' = 'medium') => formatDate(ts, i18n.locale, style),
  dateTime: (ts: number) => formatDateTime(ts, i18n.locale),
  when: (ts: number, now = i18n.now) => formatWhen(ts, now, i18n.locale, tk),
  ago: (ts: number, now = i18n.now) => formatAgo(ts, now, i18n.locale, tk),
  duration: (ms: number) => formatDuration(ms, tk),
};
