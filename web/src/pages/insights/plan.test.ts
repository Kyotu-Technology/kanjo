import { expect, test } from 'vitest';
import en from '../../lib/i18n/en';
import { nextStepKey } from './plan';

test('the top plan with frequent limit hits gets advice, not "nothing to do"', () => {
  expect(nextStepKey({ action: 'keep', reason: 'frequent_limit_hits' })).toBe('keep_hot');
  expect(nextStepKey({ action: 'keep', reason: 'within_plan' })).toBe('keep');
  expect(nextStepKey({ action: 'unknown', reason: 'no_history' })).toBe('no_history');
  expect(nextStepKey({ action: 'upgrade', reason: 'frequent_limit_hits' })).toBe('upgrade');
});

test('every next-step key exists in the dictionary', () => {
  const keys = Object.keys(en.pages.insights.plan.do);
  for (const action of ['keep', 'upgrade', 'downgrade', 'unknown'] as const)
    for (const reason of ['within_plan', 'frequent_limit_hits', 'low_utilisation', 'api_cheaper', 'no_plan', 'no_history']) expect(keys).toContain(nextStepKey({ action, reason }));
});
