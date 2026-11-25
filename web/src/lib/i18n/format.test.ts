import { describe, expect, test } from 'vitest';
import { translate } from './core';
import en from './en';
import pl from './pl';
import { daysBetween, durationParts, formatCompact, formatDuration, formatMoney, formatPct, formatWhen } from './format';

const norm = (s: string) => s.replace(/[\u{a0}\u{202f}]/gu, ' ');
const tEn = (k: string, p?: Record<string, string | number>) => translate(en, 'en-GB', k, p);
const tPl = (k: string, p?: Record<string, string | number>) => translate(pl, 'pl-PL', k, p);

describe('money', () => {
  test('USD in English', () => {
    expect(formatMoney(12.345, { code: 'USD', usd_rate: 1 }, 'en-GB')).toBe('$12.35');
  });

  test('converts with the manual rate and uses the display currency', () => {
    expect(norm(formatMoney(10, { code: 'PLN', usd_rate: 4 }, 'pl-PL'))).toBe('40,00 zł');
  });

  test('drops cents above a thousand and compacts on request', () => {
    expect(norm(formatMoney(1234.5, { code: 'EUR', usd_rate: 1 }, 'pl-PL'))).toBe('1235 €');
    expect(formatMoney(12_400, { code: 'USD', usd_rate: 1 }, 'en-GB', { compact: true })).toMatch(/^\$12\.4k$/i);
  });

  test('an invalid currency code still renders', () => {
    expect(formatMoney(5, { code: 'XX', usd_rate: 1 }, 'en-GB')).toBe('5.00 XX');
  });

  test('a broken rate falls back to 1', () => {
    expect(formatMoney(5, { code: 'USD', usd_rate: 0 }, 'en-GB')).toBe('$5.00');
  });
});

describe('numbers', () => {
  test('percent takes 0..100', () => {
    expect(formatPct(46.4, 'en-GB')).toBe('46%');
    expect(norm(formatPct(46.4, 'pl-PL'))).toBe('46%');
  });

  test('compact keeps small numbers whole', () => {
    expect(formatCompact(9_999, 'en-GB')).toBe('9,999');
    expect(formatCompact(1_250_000, 'en-GB')).toMatch(/^1\.3m$/i);
  });
});

describe('time phrases', () => {
  const now = new Date(2026, 10, 26, 10, 0).getTime();

  test('durations', () => {
    expect(durationParts(2 * 3_600_000 + 13 * 60_000)).toEqual({ d: 0, h: 2, m: 13 });
    expect(formatDuration(2 * 3_600_000 + 13 * 60_000, tEn)).toBe('2 h 13 min');
    expect(formatDuration(2 * 3_600_000 + 13 * 60_000, tPl)).toBe('2 godz. 13 min');
    expect(formatDuration(20_000, tEn)).toBe('less than a minute');
  });

  test('weekday phrases use the right Polish preposition', () => {
    const tue = new Date(2026, 11, 1, 9, 0).getTime();
    const later = new Date(2026, 11, 3, 14, 0).getTime();
    expect(daysBetween(now, tue)).toBe(5);
    expect(formatWhen(tue, now, 'pl-PL', tPl)).toBe('we wtorek o 09:00');
    expect(formatWhen(later, now, 'en-GB', tEn)).toBe('on 3 December at 14:00');
  });

  test('today and tomorrow', () => {
    expect(formatWhen(now + 4 * 3_600_000, now, 'en-GB', tEn)).toBe('today at 14:00');
    expect(formatWhen(now + 24 * 3_600_000, now, 'pl-PL', tPl)).toBe('jutro o 10:00');
  });

  test('a weekday this week', () => {
    const sat = new Date(2026, 10, 28, 14, 0).getTime();
    expect(formatWhen(sat, now, 'en-GB', tEn)).toBe('on Saturday at 14:00');
    expect(formatWhen(sat, now, 'pl-PL', tPl)).toBe('w sobotę o 14:00');
  });
});
