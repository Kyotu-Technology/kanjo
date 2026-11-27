import { t } from './i18n/index.svelte';
import { app } from './stores/app.svelte';

export const titlesOff = () => app.settings?.privacy.session_titles === false;

export const untitled = () => t(titlesOff() ? 'dashboard.sessions.untitled_off' : 'dashboard.sessions.untitled');
