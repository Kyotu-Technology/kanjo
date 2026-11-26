import type { BreakdownRow } from '../../lib/api/contract';

/** The costliest rows of a block breakdown and how many more there are. */
export function topRows(rows: BreakdownRow[], n = 3): { top: BreakdownRow[]; more: number } {
  const sorted = [...rows].filter((r) => r.usage.cost_usd > 0 || r.usage.requests > 0).sort((a, b) => b.usage.cost_usd - a.usage.cost_usd);
  return { top: sorted.slice(0, n), more: Math.max(0, sorted.length - n) };
}
