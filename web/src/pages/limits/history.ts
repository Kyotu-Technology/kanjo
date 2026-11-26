import type { LimitPoint } from '../../lib/api/contract';

/**
 * Chart points for limit readings. Use within a window only grows, so between the last
 * reading and the reset the line holds its last value, then drops to zero at the
 * reset, instead of sloping down through hours that had no readings.
 */
export function limitLine(raw: LimitPoint[], now: number): { x: number; y: number }[] {
  const out: { x: number; y: number }[] = [];
  raw.forEach((p, i) => {
    out.push({ x: p.ts, y: p.used_pct });
    const next = raw[i + 1];
    const ended = (!next || next.resets_at !== p.resets_at) && p.resets_at <= now && (!next || p.resets_at <= next.ts);
    if (ended && p.resets_at > p.ts) out.push({ x: p.resets_at, y: p.used_pct }, { x: p.resets_at + 1, y: 0 });
  });
  return out.sort((a, b) => a.x - b.x);
}
