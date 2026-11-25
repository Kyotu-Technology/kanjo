import type { Theme } from '../api/contract';

export type ResolvedTheme = 'light' | 'dark';

export function resolveTheme(pref: Theme, prefersDark: boolean): ResolvedTheme {
  return pref === 'system' ? (prefersDark ? 'dark' : 'light') : pref;
}

class ThemeState {
  pref = $state<Theme>('system');
  resolved = $state<ResolvedTheme>('light');
  private mq: MediaQueryList | undefined;

  constructor() {
    if (typeof window === 'undefined') return;
    this.mq = window.matchMedia('(prefers-color-scheme: dark)');
    this.mq.addEventListener('change', () => this.apply(this.pref));
    this.resolved = (document.documentElement.dataset.theme as ResolvedTheme) ?? 'light';
  }

  apply(pref: Theme): void {
    this.pref = pref;
    this.resolved = resolveTheme(pref, this.mq?.matches ?? false);
    if (typeof document !== 'undefined') document.documentElement.dataset.theme = this.resolved;
    try {
      localStorage.setItem('kanjo.theme', pref);
    } catch {
      // storage can be unavailable in private windows; the theme still applies
    }
  }
}

export const theme = new ThemeState();
