import { expect, test } from 'vitest';
import { lastActivity } from './util';

test('after a restart the newest stored request counts as the last activity', () => {
  expect(lastActivity(null, 1_000)).toBe(1_000);
  expect(lastActivity(2_000, 1_000)).toBe(2_000);
  expect(lastActivity(null, null)).toBeNull();
  expect(lastActivity(undefined, undefined)).toBeNull();
});
