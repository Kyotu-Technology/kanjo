import type { PlanKind } from '../../lib/api/contract';

/** List prices in USD per month, as in the onboarding wizard; null where there is no fixed price. */
export const PLANS: { kind: PlanKind; price: number | null; seat?: boolean }[] = [
  { kind: 'pro', price: 20 },
  { kind: 'max', price: 100 },
  { kind: 'ultra', price: 200 },
  { kind: 'team', price: 25, seat: true },
  { kind: 'team_premium', price: 125, seat: true },
  { kind: 'enterprise', price: null },
  { kind: 'api', price: null },
  { kind: 'custom', price: null },
  { kind: 'unknown', price: null },
];

export function listPrice(kind: PlanKind): number | null {
  return PLANS.find((p) => p.kind === kind)?.price ?? null;
}
