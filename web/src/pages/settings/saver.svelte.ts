import type { Settings } from '../../lib/api/contract';
import { cache, updateSettings } from '../../lib/stores/app.svelte';

export type SaveState = 'idle' | 'saving' | 'saved' | 'failed';

/** Save state of one settings section: saving, saved (for a few seconds) or failed and rolled back. */
export class Saver {
  state = $state<SaveState>('idle');
  private timer: ReturnType<typeof setTimeout> | undefined;

  async save(change: (s: Settings) => Settings, tags: string[] = []): Promise<boolean> {
    clearTimeout(this.timer);
    this.state = 'saving';
    const ok = await updateSettings(change);
    this.state = ok ? 'saved' : 'failed';
    if (ok) cache.invalidate(['settings', 'system', ...tags]);
    this.timer = setTimeout(() => (this.state = 'idle'), ok ? 3500 : 8000);
    return ok;
  }
}

const MARKER = '@@kanjo-slot@@';

/** Splits a translated sentence around one placeholder, so a component can sit inside it. */
export function around(text: (marker: string) => string): [string, string] {
  const [a, b = ''] = text(MARKER).split(MARKER);
  return [a, b];
}

/** Parses a decimal typed with a comma or a dot; NaN when it is not a plain number. */
export function parseDecimal(s: string): number {
  const v = s.trim().replace(/\s/g, '').replace(',', '.');
  return /^\d+(\.\d+)?$/.test(v) ? Number(v) : NaN;
}

export function goToSection(id: string): void {
  const el = document.getElementById(id);
  if (!el) return;
  const box = el.closest<HTMLElement>('.card') ?? el;
  box.scrollIntoView({ behavior: 'smooth', block: 'start' });
  if (!el.hasAttribute('tabindex')) el.tabIndex = -1;
  el.focus({ preventScroll: true });
}
