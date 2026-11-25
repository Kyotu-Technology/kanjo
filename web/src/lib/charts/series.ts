/**
 * Categorical colours follow the entity, never its rank: each model family
 * owns a slot, other models take the next free slot in order of first
 * appearance and keep it for the session. The tail folds into "Other".
 */
const FAMILY_SLOT: Record<string, number> = { opus: 1, sonnet: 2, haiku: 3 };
const MAX_SLOT = 6;
const assigned = new Map<string, number>();

export const OTHER = '__other__';

function family(model: string): string | null {
  const m = /(opus|sonnet|haiku)/i.exec(model);
  return m ? m[1].toLowerCase() : null;
}

export function modelSlot(model: string): number | null {
  const f = family(model);
  if (f) return FAMILY_SLOT[f];
  const known = assigned.get(model);
  if (known) return known;
  const used = new Set(assigned.values());
  for (let s = 4; s <= MAX_SLOT; s++) {
    if (!used.has(s)) {
      assigned.set(model, s);
      return s;
    }
  }
  return null;
}

export function seriesColor(slot: number | null): string {
  return slot === null ? 'var(--series-other)' : `var(--series-${slot})`;
}

export function modelColor(model: string): string {
  return model === OTHER ? seriesColor(null) : seriesColor(modelSlot(model));
}

/** "claude-opus-4-5" -> "Opus 4.5"; unknown names are returned unchanged. */
export function modelLabel(model: string): string {
  const m = /^claude-(opus|sonnet|haiku)-(\d+)(?:-(\d+))?/i.exec(model);
  if (!m) return model;
  const name = m[1][0].toUpperCase() + m[1].slice(1).toLowerCase();
  return m[3] ? `${name} ${m[2]}.${m[3]}` : `${name} ${m[2]}`;
}

export interface Series {
  key: string;
  label: string;
  color: string;
}

/**
 * Series for a set of models ranked by total, at most `max` named series;
 * the rest fold into OTHER. `fold` maps a model to its series key.
 */
export function modelSeries(totals: Map<string, number>, otherLabel: string, max = 4): { series: Series[]; fold: (model: string) => string } {
  const ranked = [...totals.entries()].sort((a, b) => b[1] - a[1]).map(([m]) => m);
  const named = ranked.filter((m) => modelSlot(m) !== null).slice(0, max);
  const keep = new Set(named);
  const ordered = [...named].sort((a, b) => (modelSlot(a) ?? 99) - (modelSlot(b) ?? 99));
  const series: Series[] = ordered.map((m) => ({ key: m, label: modelLabel(m), color: modelColor(m) }));
  if (ranked.some((m) => !keep.has(m))) series.push({ key: OTHER, label: otherLabel, color: seriesColor(null) });
  return { series, fold: (m) => (keep.has(m) ? m : OTHER) };
}
