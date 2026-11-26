import type { Dimension, Granularity } from '../../lib/api/contract';
import { PRESETS, customRange, rangeFor, toDateInput, type DateRange } from '../../lib/range';

export interface UsageFilters {
  range: DateRange;
  group: Granularity;
  by: Dimension;
  project: string;
  model: string;
  agent: string;
  client: string;
}

const GROUPS: readonly Granularity[] = ['hour', 'day', 'week', 'month'];
export const DIMS: readonly Dimension[] = ['project', 'model', 'agent', 'client', 'branch', 'session', 'skill', 'mcp', 'tool'];
export const ESTIMATED: readonly Dimension[] = ['skill', 'mcp', 'tool'];

/** Reads the page state from the hash query, so a link or a reload shows the same view. */
export function fromQuery(q: Record<string, string>, now: number): UsageFilters {
  const preset = (PRESETS as readonly string[]).includes(q.range ?? '') ? (q.range as (typeof PRESETS)[number]) : null;
  const range = preset ? rangeFor(preset, now) : (q.from && q.to ? customRange(q.from, q.to) : null) ?? rangeFor('last_30', now);
  return {
    range,
    group: (GROUPS as readonly string[]).includes(q.group ?? '') ? (q.group as Granularity) : 'day',
    by: (DIMS as readonly string[]).includes(q.by ?? '') ? (q.by as Dimension) : 'project',
    project: q.project ?? '',
    model: q.model ?? '',
    agent: q.agent ?? '',
    client: q.client ?? '',
  };
}

export function toQuery(f: UsageFilters): Record<string, string | undefined> {
  const custom = f.range.preset === 'custom';
  return {
    range: custom || f.range.preset === 'last_30' ? undefined : f.range.preset,
    from: custom ? toDateInput(f.range.from) : undefined,
    to: custom ? toDateInput(f.range.to - 1) : undefined,
    group: f.group === 'day' ? undefined : f.group,
    by: f.by === 'project' ? undefined : f.by,
    project: f.project || undefined,
    model: f.model || undefined,
    agent: f.agent || undefined,
    client: f.client || undefined,
  };
}

/** The API filter parameters of the page state. */
export function apiFilters(f: Pick<UsageFilters, 'project' | 'model' | 'agent' | 'client'>) {
  return {
    project: f.project ? Number(f.project) : undefined,
    model: f.model || undefined,
    agent: f.agent || undefined,
    client: f.client || undefined,
  };
}
