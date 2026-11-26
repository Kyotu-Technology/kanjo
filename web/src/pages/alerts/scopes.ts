import type { Budget, BudgetScope } from '../../lib/api/contract';

/**
 * The scopes the budget dialog offers. Kanjo shows no account IDs anywhere, so a
 * new budget cannot be tied to an account; a budget that already has that scope keeps it.
 */
export function scopes(budget: Pick<Budget, 'scope'> | null): BudgetScope[] {
  const base: BudgetScope[] = ['global', 'project', 'client'];
  return budget?.scope === 'account' ? [...base, 'account'] : base;
}
