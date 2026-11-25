import { matchRoute, type Route } from '../router';

class RouterState {
  current = $state<Route>(matchRoute(typeof location === 'undefined' ? '' : location.hash));

  constructor() {
    if (typeof window === 'undefined') return;
    window.addEventListener('hashchange', () => {
      this.current = matchRoute(location.hash);
    });
  }
}

export const router = new RouterState();

export function navigate(hash: string): void {
  if (location.hash === hash) return;
  location.hash = hash;
}
