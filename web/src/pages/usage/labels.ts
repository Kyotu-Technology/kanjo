import type { BreakdownRow, Dimension, UsageBucket, Granularity } from '../../lib/api/contract';
import { modelLabel } from '../../lib/charts/series';
import { fmt, i18n, t } from '../../lib/i18n/index.svelte';

/** Human label of a breakdown row in its dimension. */
export function rowLabel(by: Dimension, r: Pick<BreakdownRow, 'key' | 'label'>): string {
  if (by === 'model') return modelLabel(r.label || r.key);
  if (by === 'agent') return r.key === 'main' ? t('common.main') : r.label;
  if (by === 'client') return r.key ? r.label : t('pages.usage.filters.no_client');
  return r.label || t('common.none');
}

const monthFmt = new Map<string, Intl.DateTimeFormat>();
function month(ts: number, style: 'long' | 'short'): string {
  const key = i18n.locale + style;
  let f = monthFmt.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(i18n.locale, style === 'long' ? { month: 'long', year: 'numeric' } : { month: 'short' });
    monthFmt.set(key, f);
  }
  return f.format(ts);
}

/** Tooltip title and axis tick of a usage bucket. */
export function bucketText(b: UsageBucket, group: Granularity): { label: string; tick: string } {
  switch (group) {
    case 'hour':
      return { label: fmt.dateTime(b.start), tick: new Date(b.start).getHours() === 0 ? fmt.date(b.start, 'short') : fmt.time(b.start) };
    case 'day':
      return { label: fmt.date(b.start, 'long'), tick: fmt.date(b.start, 'short') };
    case 'week':
      return { label: t('pages.usage.week_of', { date: fmt.date(b.start, 'medium') }), tick: fmt.date(b.start, 'short') };
    case 'month':
      return { label: month(b.start, 'long'), tick: month(b.start, 'short') };
  }
}

export function rangeText(from: number, to: number): { from: string; to: string } {
  const sameYear = new Date(from).getFullYear() === new Date(to - 1).getFullYear() && new Date(to - 1).getFullYear() === new Date(i18n.now).getFullYear();
  const style = sameYear ? 'medium' : 'long';
  return { from: fmt.date(from, style), to: fmt.date(to - 1, style) };
}
