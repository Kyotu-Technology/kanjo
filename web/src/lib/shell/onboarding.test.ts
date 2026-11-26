import { expect, test } from 'vitest';
import { connectState } from './onboarding';

test('a demo server only explains the Connect step, whatever its hook status says', () => {
  expect(connectState(true, false)).toBe('demo');
  expect(connectState(true, true)).toBe('demo');
  expect(connectState(false, false)).toBe('open');
  expect(connectState(false, true)).toBe('connected');
});
