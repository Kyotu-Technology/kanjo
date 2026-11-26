import { expect, test } from 'vitest';
import { scopes } from './scopes';

test('a new budget cannot pick an account, an existing account budget keeps its scope', () => {
  expect(scopes(null)).not.toContain('account');
  expect(scopes({ scope: 'project' })).not.toContain('account');
  expect(scopes({ scope: 'account' })).toContain('account');
});
