import { i18n } from '../../lib/i18n/index.svelte';

/** "2026-11" as a short month name with the year, in the current locale. */
export function monthLabel(m: string): string {
  const [y, mo] = m.split('-').map(Number);
  return new Intl.DateTimeFormat(i18n.locale, { month: 'short', year: 'numeric' }).format(new Date(y, mo - 1, 1));
}
