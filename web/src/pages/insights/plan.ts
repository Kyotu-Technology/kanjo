import type { PlanFit } from '../../lib/api/contract';

/** The dictionary key of the next step under pages.insights.plan.do, matched to what the verdict says. */
export function nextStepKey(p: Pick<PlanFit, 'action' | 'reason'>): string {
  if (p.action === 'unknown' && p.reason === 'no_history') return 'no_history';
  if (p.action === 'keep' && p.reason === 'frequent_limit_hits') return 'keep_hot';
  return p.action;
}
