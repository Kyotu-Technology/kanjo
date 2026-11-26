import type { Alert } from '../../lib/api/contract';
import { href } from '../../lib/router';

export type AlertLink = { kind: 'session' | 'limits' | 'insights' | 'usage'; href: string } | { kind: 'budget'; id: number };

/** Where an alert leads: the session or budget it names, else the page that explains it. */
export function alertLink(a: Alert): AlertLink | null {
  const p = (a.params ?? {}) as Record<string, unknown>;
  if (typeof p.session_id === 'string' && p.session_id) return { kind: 'session', href: href(`/sessions/${encodeURIComponent(p.session_id)}`) };
  switch (a.kind) {
    case 'budget_warning':
    case 'budget_exceeded':
      return typeof p.budget_id === 'number' ? { kind: 'budget', id: p.budget_id } : null;
    case 'limit_threshold':
    case 'limit_forecast':
      return { kind: 'limits', href: href('/limits') };
    case 'cost_spike':
      return { kind: 'usage', href: href('/usage') };
    default:
      return { kind: 'insights', href: href('/insights') };
  }
}
