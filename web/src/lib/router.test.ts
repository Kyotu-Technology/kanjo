import { expect, test } from 'vitest';
import { href, matchRoute } from './router';

test('matches routes with params and query', () => {
  expect(matchRoute('').name).toBe('dashboard');
  expect(matchRoute('#/').name).toBe('dashboard');
  expect(matchRoute('#/usage?from=1&group=week')).toEqual({ name: 'usage', path: '/usage', params: {}, query: { from: '1', group: 'week' } });
  expect(matchRoute('#/sessions/ses%2F1').params.id).toBe('ses/1');
  expect(matchRoute('#/sessions/').name).toBe('sessions');
  expect(matchRoute('#/nope').name).toBe('not_found');
});

test('builds hash links', () => {
  expect(href('/usage', { model: 'claude-opus-4-5', from: undefined })).toBe('#/usage?model=claude-opus-4-5');
  expect(href('/')).toBe('#/');
});
