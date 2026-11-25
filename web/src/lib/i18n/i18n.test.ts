import { describe, expect, test } from 'vitest';
import { flatten, isPlural, placeholders, translate, type Entry } from './core';
import en from './en';
import pl from './pl';

const texts = (e: Entry): string[] => (isPlural(e) ? [e.one, e.few, e.many, e.other].filter((x): x is string => x !== undefined) : [e]);

describe('dictionaries', () => {
  const enMap = new Map(flatten(en));
  const plMap = new Map(flatten(pl));

  test('Polish has every English key and nothing extra', () => {
    const missing = [...enMap.keys()].filter((k) => !plMap.has(k));
    const extra = [...plMap.keys()].filter((k) => !enMap.has(k));
    expect(missing).toEqual([]);
    expect(extra).toEqual([]);
  });

  test('no entry is empty', () => {
    for (const [lang, map] of [['en', enMap], ['pl', plMap]] as const) {
      for (const [k, v] of map) for (const s of texts(v)) expect(s.trim(), `${lang}:${k}`).not.toBe('');
    }
  });

  test('placeholders match between languages', () => {
    for (const [k, v] of enMap) {
      const p = plMap.get(k);
      if (!p) continue;
      const want = [...new Set(texts(v).flatMap(placeholders))].sort().join(',');
      const got = [...new Set(texts(p).flatMap(placeholders))].sort().join(',');
      expect(got, `pl:${k}`).toBe(want);
    }
  });

  test('plurals are plural in both languages and Polish has one, few and many', () => {
    for (const [k, v] of enMap) {
      const p = plMap.get(k);
      expect(isPlural(p), k).toBe(isPlural(v));
      if (isPlural(p)) expect(Boolean(p.one && p.few && p.many && p.other), k).toBe(true);
    }
  });

  test('Polish text uses diacritics', () => {
    const all = [...plMap.values()].flatMap(texts).join(' ');
    expect(all).toMatch(/[ąćęłńóśźż]/);
  });

  test('no long dashes or typographic arrows in copy', () => {
    for (const map of [enMap, plMap]) for (const [k, v] of map) for (const s of texts(v)) expect(s, k).not.toMatch(/[\u{2013}\u{2014}\u{2190}\u{2192}\u{21d2}]/u);
  });
});

describe('translate', () => {
  test('interpolates and falls back to the key', () => {
    expect(translate(en, 'en-GB', 'common.updated', { ago: '2 minutes ago' })).toBe('Updated 2 minutes ago');
    expect(translate(en, 'en-GB', 'does.not.exist')).toBe('does.not.exist');
  });

  test('selects Polish plural forms', () => {
    const f = (n: number) => translate(pl, 'pl-PL', 'units.sessions', { count: n });
    expect(f(1)).toBe('1 sesja');
    expect(f(3)).toBe('3 sesje');
    expect(f(5)).toBe('5 sesji');
    expect(f(22)).toBe('22 sesje');
    expect(f(12)).toBe('12 sesji');
  });

  test('countLabel replaces the shown count', () => {
    expect(translate(en, 'en-GB', 'units.requests', { count: 1200, countLabel: '1,200' })).toBe('1,200 requests');
  });

  test('falls back to English when a language misses a key', () => {
    expect(translate({}, 'pl-PL', 'common.retry', undefined, en)).toBe('Try again');
  });
});
