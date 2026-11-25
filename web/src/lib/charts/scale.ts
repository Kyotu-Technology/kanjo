/** Round axis maximum and evenly spaced ticks (0 / 25 / 50 ...). */
export function niceTicks(max: number, count = 4): number[] {
  if (!(max > 0)) return [0, 1];
  const raw = max / count;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const norm = raw / mag;
  const step = (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 2.5 ? 2.5 : norm <= 5 ? 5 : 10) * mag;
  const top = Math.ceil(max / step - 1e-9) * step;
  const ticks: number[] = [];
  for (let v = 0; v <= top + step / 2; v += step) ticks.push(Math.round(v * 1e6) / 1e6);
  return ticks;
}

export function linear(d0: number, d1: number, r0: number, r1: number): (v: number) => number {
  const span = d1 - d0 || 1;
  return (v) => r0 + ((v - d0) / span) * (r1 - r0);
}

/** Path for a bar with rounded top corners and a square base. */
export function roundedTop(x: number, y: number, w: number, h: number, r = 4): string {
  if (h <= 0 || w <= 0) return '';
  const rr = Math.min(r, w / 2, h);
  return `M${x},${y + h}V${y + rr}Q${x},${y} ${x + rr},${y}H${x + w - rr}Q${x + w},${y} ${x + w},${y + rr}V${y + h}Z`;
}

/** Index of the label slots to print so labels do not collide. */
export function labelEvery(n: number, width: number, minPx = 56): number {
  if (n <= 0) return 1;
  return Math.max(1, Math.ceil(n / Math.max(1, Math.floor(width / minPx))));
}

/** Sequential step 0..6 for a value against the maximum; 0 means none. */
export function seqStep(v: number, max: number, steps = 6): number {
  if (!(v > 0) || !(max > 0)) return 0;
  return Math.max(1, Math.min(steps, Math.ceil((v / max) * steps)));
}
