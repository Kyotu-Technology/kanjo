/**
 * Categorical colours follow the entity, never its rank: each model family
 * owns a slot, other models take the next free slot in order of first
 * appearance and keep it for the session. The tail folds into "Other".
 */
const FAMILY_SLOT: Record<string, number> = { opus: 1, sonnet: 2, haiku: 3 };
const MAX_SLOT = 6;
const assigned = new Map<string, number>();

export const OTHER = '__other__';

/**
 * Reads a Claude model id by rule: "claude-<family>-<major>[-<minor>][-<yyyymmdd>]"
 * and the older "claude-<major>[-<minor>]-<family>[-<yyyymmdd>]". Anything else is not a Claude id.
 */
function parseModel(model: string): { family: string; version: string } | null {
  const a = /^claude-([a-z]+)-(\d{1,2})(?:-(\d{1,2}))?(?:-\d{8})?$/i.exec(model);
  const b = a ? null : /^claude-(\d{1,2})(?:-(\d{1,2}))?-([a-z]+)(?:-\d{8})?$/i.exec(model);
  const [fam, major, minor] = a ? [a[1], a[2], a[3]] : b ? [b[3], b[1], b[2]] : [];
  if (!fam || !major) return null;
  return { family: fam.toLowerCase(), version: minor ? `${major}.${minor}` : major };
}

function family(model: string): string | null {
  return parseModel(model)?.family ?? null;
}

export function modelSlot(model: string): number | null {
  const f = family(model);
  if (f && FAMILY_SLOT[f]) return FAMILY_SLOT[f];
  const key = f ?? model;
  const known = assigned.get(key);
  if (known) return known;
  const used = new Set(assigned.values());
  for (let s = 4; s <= MAX_SLOT; s++) {
    if (!used.has(s)) {
      assigned.set(key, s);
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

/** "claude-opus-4-5" -> "Opus 4.5", "claude-fable-5-2" -> "Fable 5.2"; other names are returned unchanged. */
export function modelLabel(model: string): string {
  const p = parseModel(model);
  if (!p) return model;
  return `${p.family[0].toUpperCase()}${p.family.slice(1)} ${p.version}`;
}

/** The raw model id for a tooltip, when the friendly name hides it. */
export function modelTitle(model: string): string | undefined {
  return modelLabel(model) === model ? undefined : model;
}

export interface Series {
  key: string;
  label: string;
  color: string;
  title?: string;
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
  const series: Series[] = ordered.map((m) => ({ key: m, label: modelLabel(m), color: modelColor(m), title: modelTitle(m) }));
  if (ranked.some((m) => !keep.has(m))) series.push({ key: OTHER, label: otherLabel, color: seriesColor(null) });
  return { series, fold: (m) => (keep.has(m) ? m : OTHER) };
}
