import type { CurrencySettings } from '../api/contract';

export type Lang = 'en' | 'pl';
export type DayKey = 'mon' | 'tue' | 'wed' | 'thu' | 'fri' | 'sat' | 'sun';

export const DAY_KEYS: readonly DayKey[] = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];

/** English uses British conventions (24-hour clock, day before month), matching the product's examples. */
export function intlLocale(lang: Lang): string {
  return lang === 'pl' ? 'pl-PL' : 'en-GB';
}

const nfCache = new Map<string, Intl.NumberFormat>();

function nf(locale: string, opts: Intl.NumberFormatOptions): Intl.NumberFormat {
  const key = locale + JSON.stringify(opts);
  let f = nfCache.get(key);
  if (!f) {
    f = new Intl.NumberFormat(locale, opts);
    nfCache.set(key, f);
  }
  return f;
}

export interface MoneyOptions {
  /** 12.3K style, for axes and tight spaces. */
  compact?: boolean;
  /** Fixed number of fraction digits; by default cents below 1000 units and whole units above. */
  digits?: number;
}

/** Converts USD with the user's manual rate and formats it in their display currency. */
export function formatMoney(usd: number, currency: CurrencySettings, locale: string, opts: MoneyOptions = {}): string {
  const rate = Number.isFinite(currency.usd_rate) && currency.usd_rate > 0 ? currency.usd_rate : 1;
  const value = usd * rate;
  const abs = Math.abs(value);
  const digits = opts.digits ?? (opts.compact ? (abs >= 1000 ? 1 : abs >= 100 ? 0 : abs >= 10 ? 0 : 2) : abs >= 1000 ? 0 : 2);
  const base: Intl.NumberFormatOptions = {
    style: 'currency',
    currency: currency.code || 'USD',
    currencyDisplay: 'narrowSymbol',
    minimumFractionDigits: opts.compact && abs >= 1000 ? 0 : digits,
    maximumFractionDigits: digits,
    notation: opts.compact && abs >= 1000 ? 'compact' : 'standard',
  };
  try {
    return nf(locale, base).format(value);
  } catch {
    const n = nf(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
    return `${n} ${currency.code}`;
  }
}

export function formatNumber(n: number, locale: string, digits = 0): string {
  return nf(locale, { maximumFractionDigits: digits, minimumFractionDigits: 0 }).format(n);
}

export function formatCompact(n: number, locale: string): string {
  if (Math.abs(n) < 10_000) return formatNumber(n, locale);
  return nf(locale, { notation: 'compact', maximumFractionDigits: 1 }).format(n);
}

/** `pct` is 0..100 (API convention for *_pct fields). */
export function formatPct(pct: number, locale: string, digits = 0): string {
  return nf(locale, { style: 'percent', maximumFractionDigits: digits, minimumFractionDigits: 0 }).format(pct / 100);
}

/** `ratio` is 0..1 (API convention for *_ratio and share fields). */
export function formatRatio(ratio: number, locale: string, digits = 0): string {
  return formatPct(ratio * 100, locale, digits);
}

const dtfCache = new Map<string, Intl.DateTimeFormat>();

function dtf(locale: string, opts: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = locale + JSON.stringify(opts);
  let f = dtfCache.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(locale, opts);
    dtfCache.set(key, f);
  }
  return f;
}

export function formatTime(ts: number, locale: string): string {
  return dtf(locale, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(ts);
}

export function formatHour(ts: number, locale: string): string {
  return dtf(locale, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(ts);
}

export function formatDate(ts: number, locale: string, style: 'short' | 'medium' | 'long' = 'medium'): string {
  const opts: Intl.DateTimeFormatOptions =
    style === 'short'
      ? { day: 'numeric', month: 'short' }
      : style === 'medium'
        ? { day: 'numeric', month: 'long' }
        : { day: 'numeric', month: 'long', year: 'numeric' };
  return dtf(locale, opts).format(ts);
}

export function formatWeekday(ts: number, locale: string): string {
  return dtf(locale, { weekday: 'long' }).format(ts);
}

export function formatDateTime(ts: number, locale: string): string {
  return dtf(locale, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(ts);
}

/** Local weekday of a timestamp as a dictionary key. */
export function dayKey(ts: number): DayKey {
  return DAY_KEYS[(new Date(ts).getDay() + 6) % 7];
}

export function startOfLocalDay(ts: number): number {
  const d = new Date(ts);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** Whole local calendar days from `now` to `ts` (0 = same day). */
export function daysBetween(now: number, ts: number): number {
  return Math.round((startOfLocalDay(ts) - startOfLocalDay(now)) / 86_400_000);
}

export interface DurationParts {
  d: number;
  h: number;
  m: number;
}

export function durationParts(ms: number): DurationParts {
  const totalMin = Math.max(0, Math.floor(ms / 60_000));
  return { d: Math.floor(totalMin / 1440), h: Math.floor((totalMin % 1440) / 60), m: totalMin % 60 };
}

export type Translate = (key: string, params?: Record<string, string | number>) => string;

/** "2 h 13 min", "1 d 4 h", "45 min". */
export function formatDuration(ms: number, t: Translate): string {
  if (ms < 60_000) return t('units.less_than_min');
  const { d, h, m } = durationParts(ms);
  if (d > 0) return t('units.d_h', { d, h });
  if (h > 0) return t('units.h_min', { h, m });
  return t('units.min', { m });
}

/**
 * A moment phrased for a sentence: "today at 14:00", "on Thursday at 14:00",
 * "on 3 December at 09:00". Polish weekday phrases come from the dictionary
 * because they need the preposition and the accusative ("we wtorek", "w środę").
 */
export function formatWhen(ts: number, now: number, locale: string, t: Translate): string {
  const days = daysBetween(now, ts);
  const time = formatTime(ts, locale);
  let when: string;
  if (days === 0) when = t('time.today');
  else if (days === 1) when = t('time.tomorrow');
  else if (days === -1) when = t('time.yesterday');
  else if (days > 1 && days < 7) when = t(`time.on_day.${dayKey(ts)}`);
  else when = t('time.on_date', { date: formatDate(ts, locale, 'medium') });
  return t('time.when_at', { when, time });
}

export function formatAgo(ts: number, now: number, locale: string, t: Translate): string {
  const diff = ts - now;
  const abs = Math.abs(diff);
  if (abs < 45_000) return t('time.just_now');
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  if (abs < 3_600_000) return rtf.format(Math.round(diff / 60_000), 'minute');
  if (abs < 86_400_000) return rtf.format(Math.round(diff / 3_600_000), 'hour');
  return rtf.format(Math.round(diff / 86_400_000), 'day');
}
