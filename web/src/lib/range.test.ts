import { expect, test } from 'vitest';
import { customRange, rangeFor, toDateInput } from './range';

const now = new Date(2026, 10, 26, 15, 30).getTime();

test('presets cover whole local days and end tomorrow at midnight', () => {
  const r = rangeFor('last_7', now);
  expect(toDateInput(r.from)).toBe('2026-11-20');
  expect(toDateInput(r.to)).toBe('2026-11-27');
  expect(toDateInput(rangeFor('month_to_date', now).from)).toBe('2026-11-01');
  const lm = rangeFor('last_month', now);
  expect([toDateInput(lm.from), toDateInput(lm.to)]).toEqual(['2026-10-01', '2026-11-01']);
});

test('custom ranges include the end day and reject reversed input', () => {
  const r = customRange('2026-11-01', '2026-11-03');
  expect(r && toDateInput(r.to)).toBe('2026-11-04');
  expect(customRange('2026-11-05', '2026-11-03')).toBeNull();
  expect(customRange('nope', '2026-11-03')).toBeNull();
});
