import { expect, test } from 'vitest';
import type { Alert } from '../../lib/api/contract';
import { matchRoute } from '../../lib/router';
import { alertLink } from './links';

const alert = (kind: Alert['kind'], params: Record<string, unknown>): Alert => ({ id: 1, ts: 0, kind, severity: 'warning', params: params as Alert['params'], title: '', message: '', acknowledged_at: null });

test('an alert about a session opens it', () => {
  expect(alertLink(alert('runaway_session', { session_id: 'abc', project_id: 3 }))).toEqual({ kind: 'session', href: '#/sessions/abc' });
});

test('an alert about a project without a session opens the project in Sessions', () => {
  const link = alertLink(alert('cost_spike', { project_id: 3, project: 'atlas' }));
  expect(link).toEqual({ kind: 'project', href: '#/sessions?project=3' });
  if (link && 'href' in link) expect(matchRoute(link.href).query.project).toBe('3');
});

test('limit and budget alerts link to their page', () => {
  expect(alertLink(alert('limit_forecast', { window: 'five_hour', used_pct: 82 }))?.kind).toBe('limits');
  expect(alertLink(alert('budget_warning', { budget_id: 4 }))).toEqual({ kind: 'budget', id: 4 });
});
