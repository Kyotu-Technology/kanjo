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

test('every Claude family gets a name by rule, not from a list', () => {
  expect(modelLabel('claude-fable-5-2')).toBe('Fable 5.2');
  expect(modelLabel('claude-mythos-5')).toBe('Mythos 5');
  expect(modelLabel('claude-opus-5')).toBe('Opus 5');
  expect(modelLabel('claude-sonnet-4-20250514')).toBe('Sonnet 4');
  expect(modelLabel('claude-3-5-sonnet-20241022')).toBe('Sonnet 3.5');
  expect(modelLabel('claude-3-opus-20240229')).toBe('Opus 3');
  expect(modelLabel('claude-opus-4-5[1m]')).toBe('claude-opus-4-5[1m]');
  expect(modelLabel('claude-nova-preview')).toBe('claude-nova-preview');
  expect(modelLabel('')).toBe('');
});

test('versions of one family share its colour', () => {
  expect(modelColor('claude-fable-5-1')).toBe(modelColor('claude-fable-5-2'));
  expect(modelColor('claude-opus-5')).toBe('var(--series-1)');
  expect(modelColor('claude-3-5-sonnet-20241022')).toBe('var(--series-2)');
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
