import { expect, test } from 'vitest';
import { labelEvery, niceTicks, roundedTop, seqStep } from './scale';

test('nice ticks round up to clean steps', () => {
  expect(niceTicks(87)).toEqual([0, 25, 50, 75, 100]);
  expect(niceTicks(9.3)).toEqual([0, 2.5, 5, 7.5, 10]);
  expect(niceTicks(0)).toEqual([0, 1]);
  expect(niceTicks(1840)).toEqual([0, 500, 1000, 1500, 2000]);
});

test('helpers', () => {
  expect(roundedTop(0, 0, 10, 0)).toBe('');
  expect(roundedTop(0, 0, 10, 20)).toMatch(/^M0,20V4/);
  expect(labelEvery(30, 600)).toBe(3);
  expect(seqStep(0, 10)).toBe(0);
  expect(seqStep(10, 10)).toBe(6);
  expect(seqStep(0.1, 10)).toBe(1);
});
