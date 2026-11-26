import { expect, test } from 'vitest';
import { limitLine } from './history';

test('a window holds its last reading until the reset, then drops to zero', () => {
  const pts = limitLine(
    [
      { ts: 10, used_pct: 40, resets_at: 100 },
      { ts: 20, used_pct: 84, resets_at: 100 },
      { ts: 150, used_pct: 5, resets_at: 300 },
    ],
    1000,
  );
  expect(pts).toEqual([
    { x: 10, y: 40 },
    { x: 20, y: 84 },
    { x: 100, y: 84 },
    { x: 101, y: 0 },
    { x: 150, y: 5 },
    { x: 300, y: 5 },
    { x: 301, y: 0 },
  ]);
});

test('the running window is not closed', () => {
  expect(limitLine([{ ts: 10, used_pct: 40, resets_at: 100 }], 50)).toEqual([{ x: 10, y: 40 }]);
});
