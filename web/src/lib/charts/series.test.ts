import { expect, test } from 'vitest';
import { modelColor, modelLabel, modelSeries, OTHER } from './series';

test('model families keep their colour regardless of rank', () => {
  expect(modelColor('claude-sonnet-4-5')).toBe('var(--series-2)');
  expect(modelColor('claude-opus-4-5')).toBe('var(--series-1)');
  const a = modelColor('claude-nova-preview');
  expect(modelColor('claude-nova-preview')).toBe(a);
});

test('labels', () => {
  expect(modelLabel('claude-opus-4-5')).toBe('Opus 4.5');
  expect(modelLabel('claude-haiku-4-5-20251001')).toBe('Haiku 4.5');
  expect(modelLabel('gpt-x')).toBe('gpt-x');
});

test('the tail folds into Other and order follows slots', () => {
  const totals = new Map([
    ['claude-haiku-4-5', 5],
    ['claude-opus-4-5', 50],
    ['claude-sonnet-4-5', 20],
  ]);
  const { series, fold } = modelSeries(totals, 'Other', 2);
  expect(series.map((s) => s.key)).toEqual(['claude-opus-4-5', 'claude-sonnet-4-5', OTHER]);
  expect(fold('claude-haiku-4-5')).toBe(OTHER);
});
