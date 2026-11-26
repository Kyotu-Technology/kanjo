import type { IconName } from '../ui/Icon.svelte';
import type { MessageKey } from '../i18n/index.svelte';
import type { RouteName } from '../router';

export interface NavItem {
  route: RouteName;
  path: string;
  label: MessageKey;
  icon: IconName;
}

export const NAV_MAIN: NavItem[] = [
  { route: 'dashboard', path: '/', label: 'nav.dashboard', icon: 'dashboard' },
  { route: 'usage', path: '/usage', label: 'nav.usage', icon: 'usage' },
  { route: 'sessions', path: '/sessions', label: 'nav.sessions', icon: 'sessions' },
  { route: 'limits', path: '/limits', label: 'nav.limits', icon: 'limits' },
  { route: 'insights', path: '/insights', label: 'nav.insights', icon: 'insights' },
  { route: 'alerts', path: '/alerts', label: 'nav.alerts', icon: 'alerts' },
  { route: 'projects', path: '/projects', label: 'nav.projects', icon: 'projects' },
];

export const NAV_SECONDARY: NavItem[] = [
  { route: 'settings', path: '/settings', label: 'nav.settings', icon: 'settings' },
  { route: 'system', path: '/system', label: 'nav.system', icon: 'system' },
];

export const NAV_ALL = [...NAV_MAIN, ...NAV_SECONDARY];

export function navFor(route: RouteName): NavItem | undefined {
  if (route === 'session') return NAV_ALL.find((n) => n.route === 'sessions');
  return NAV_ALL.find((n) => n.route === route);
}
