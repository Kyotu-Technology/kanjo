import { fmt } from '../../lib/i18n/index.svelte';

/**
 * When the last request was recorded. The collector only knows what it wrote since it
 * started, so the database's newest request fills in after a restart.
 */
export function lastActivity(lastEventAt: number | null | undefined, lastRequestAt: number | null | undefined): number | null {
  const v = Math.max(lastEventAt ?? 0, lastRequestAt ?? 0);
  return v > 0 ? v : null;
}

export function bytes(n: number): string {
  if (n >= 1_000_000_000) return `${fmt.number(n / 1_000_000_000, 1)} GB`;
  if (n >= 1_000_000) return `${fmt.number(n / 1_000_000, 1)} MB`;
  if (n >= 1_000) return `${fmt.number(n / 1_000, 0)} kB`;
  return `${fmt.number(n)} B`;
}

export function isLoopback(bind: string): boolean {
  return bind === '127.0.0.1' || bind === '::1' || bind === 'localhost' || bind.startsWith('127.');
}

export function address(bind: string, port: number): string {
  return bind.includes(':') ? `[${bind}]:${port}` : `${bind}:${port}`;
}

/** Check rows are list items with their own id; cards are found through their heading id. */
export function scrollToSection(id: string): void {
  const el = document.getElementById(id);
  if (!el) return;
  const box = el.closest<HTMLElement>('.card') && el.tagName === 'H2' ? el.closest<HTMLElement>('.card')! : el;
  box.scrollIntoView({ behavior: 'smooth', block: 'start' });
  if (!el.hasAttribute('tabindex')) el.tabIndex = -1;
  el.focus({ preventScroll: true });
}
