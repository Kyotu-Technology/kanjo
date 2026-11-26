import { expect, test } from 'vitest';
import { translate, type Dict } from './core';
import en from './en';
import pl from './pl';

const both = [en, pl] as const;

test('the service hint names the subcommand that removes it', () => {
  for (const d of both) expect(d.system.connect.service_remove).toContain('kanjo service uninstall');
});

test('the purge dialog and the rescan help promise what purge marks deliver', () => {
  expect(en.system.purge.no_reimport).not.toMatch(/how far it has read/);
  expect(pl.system.purge.no_reimport).not.toMatch(/dokąd przeczytało/);
  expect(en.system.ingest.rescan_help).toMatch(/deleted stays deleted/);
});

test('the sign-in text does not call the permanent access key one-time', () => {
  expect(en.auth.why).not.toMatch(/one-time/i);
  expect(pl.auth.why).not.toMatch(/jednorazow/i);
});

test('Polish durations read naturally and Kanjo is neuter', () => {
  expect(translate(pl as unknown as Dict, 'pl-PL', 'units.d_h', { d: 2, h: 16 })).toBe('2 dn. 16 godz.');
  const all = JSON.stringify(pl);
  expect(all).not.toMatch(/Kanjo (nie )?[a-ząćęłńóśźż]+ł[ ,.]/);
});
