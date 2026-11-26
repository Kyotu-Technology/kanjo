import { expect, test } from 'vitest';

/**
 * The server sends style-src 'self' and script-src with one hash. Svelte sets dynamic
 * style attributes through the CSSOM, which the policy allows, but a fully static
 * style attribute is parsed from the template HTML and blocked, and so is any
 * style or script element created at run time.
 */
const sources = import.meta.glob(['./**/*.svelte', './**/*.ts', '!./**/*.test.ts', '!./lib/api/bindings/**'], { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

/** Removes every element block; repeated until nothing changes, so a block cannot reappear from the pieces around a removed one. */
function strip(src: string, block: RegExp): string {
  let prev: string;
  do {
    prev = src;
    src = src.replace(block, '');
  } while (src !== prev);
  return src;
}

test('no markup or code that the content security policy blocks', () => {
  const bad: string[] = [];
  expect(Object.keys(sources).length).toBeGreaterThan(50);
  for (const [f, src] of Object.entries(sources)) {
    const markup = f.endsWith('.svelte') ? strip(strip(src, /<script\b[\s\S]*?<\/script\b[^>]*>/gi), /<style\b[\s\S]*?<\/style\b[^>]*>/gi) : '';
    if (/\sstyle="[^"{]*"/.test(markup)) bad.push(`${f}: static style attribute`);
    if (/\{@html\s/.test(markup)) bad.push(`${f}: {@html}`);
    if (/createElement\(\s*['"](style|script)['"]/.test(src)) bad.push(`${f}: creates a style or script element`);
    if (/setAttribute\(\s*['"]style['"]|\.innerHTML\s*=|insertAdjacentHTML/.test(src)) bad.push(`${f}: writes HTML or a style attribute`);
  }
  expect(bad).toEqual([]);
});

