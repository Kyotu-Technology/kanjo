import { expect, test } from 'vitest';
import en from '../../lib/i18n/en';
import { actionKey, actorKey, actorUser, detailText } from './audit';

test('every action the server and the CLI write has a label', () => {
  const written = ['backup.create', 'backup.restore', 'data.purge', 'export', 'hooks.install', 'hooks.uninstall', 'pricing.import', 'pricing.reset', 'retention.apply', 'service.install', 'service.uninstall', 'settings.update', 'transcript.view'];
  const labels = en.system.audit.actions as Record<string, string>;
  for (const a of written) expect(labels[actionKey(a)], a).toBeTruthy();
});

test('the actor comes from how the entry was written, not from the OS user name', () => {
  expect(actorKey({ actor: 'dev', detail: { via: 'cli', changed: true } })).toBe('cli');
  expect(actorKey({ actor: 'dev', detail: { client: '127.0.0.1' } })).toBe('local');
  expect(actorKey({ actor: 'dev', detail: { client: null } })).toBe('local');
  expect(actorKey({ actor: 'dev', detail: { automatic: true } })).toBe('service');
  expect(actorKey({ actor: 'service', detail: { user: 'dev' } })).toBe('service');
  expect(actorKey({ actor: 'local', detail: {} })).toBe('local');
  expect(actorKey({ actor: 'dev', detail: {} })).toBeNull();
  expect(actorKey({ actor: 'dev', detail: null })).toBeNull();
});

test('the OS user is shown next to the role when it is known', () => {
  expect(actorUser({ actor: 'service', detail: { user: 'dev' } })).toBe('dev');
  expect(actorUser({ actor: 'dev', detail: { via: 'cli' } })).toBe('dev');
  expect(actorUser({ actor: 'local', detail: {} })).toBeNull();
  expect(actorUser({ actor: 'demo', detail: {} })).toBeNull();
});

test('details leave out the fields shown as the actor', () => {
  expect(detailText({ detail: { via: 'cli', size_bytes: 10, client: 'x' } })).toBe('size_bytes: 10');
});
