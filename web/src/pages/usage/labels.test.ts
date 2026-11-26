import { expect, test } from 'vitest';
import { sourceLabel } from './labels';

test('request sources read in plain language', () => {
  expect(sourceLabel('repl_main_thread')).toBe('Main conversation');
  expect(sourceLabel('compact')).toBe('Compaction (summary of a long conversation)');
  expect(sourceLabel('agent:custom')).toBe('Helper agent');
  expect(sourceLabel('')).toBe('Not recorded');
  expect(sourceLabel('something_new')).toBe('something_new');
});
