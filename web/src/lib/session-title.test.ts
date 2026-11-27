import { describe, expect, it } from 'vitest';
import { untitled } from './session-title';
import { app } from './stores/app.svelte';
import { defaultSettings } from './mock/data';

describe('untitled', () => {
  it('says that titles are off when they are', () => {
    const settings = defaultSettings();
    app.settings = { ...settings, privacy: { ...settings.privacy, session_titles: false } };
    expect(untitled()).toBe('Untitled: titles are off');
    app.settings = { ...settings, privacy: { ...settings.privacy, session_titles: true } };
    expect(untitled()).toBe('Untitled session');
  });
});
