import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { themePref, effectiveTheme, cycleTheme } from './theme';

beforeEach(() => {
  localStorage.clear();
  themePref.set('auto');
});

describe('theme store', () => {
  it('defaults to "auto" when nothing in localStorage', () => {
    expect(get(themePref)).toBe('auto');
  });

  it('persists pref changes to localStorage', () => {
    themePref.set('dark');
    expect(localStorage.getItem('theme-pref')).toBe('dark');
    themePref.set('light');
    expect(localStorage.getItem('theme-pref')).toBe('light');
  });

  it('effectiveTheme follows system (dark) when pref is auto', () => {
    // test-setup.ts stubs matchMedia('(prefers-color-scheme: dark)') => true
    themePref.set('auto');
    expect(get(effectiveTheme)).toBe('dark');
  });

  it('manual choice overrides system preference', () => {
    themePref.set('light');
    expect(get(effectiveTheme)).toBe('light');
    themePref.set('dark');
    expect(get(effectiveTheme)).toBe('dark');
  });

  it('cycleTheme cycles auto → light → dark → auto', () => {
    expect(get(themePref)).toBe('auto');
    cycleTheme();
    expect(get(themePref)).toBe('light');
    cycleTheme();
    expect(get(themePref)).toBe('dark');
    cycleTheme();
    expect(get(themePref)).toBe('auto');
  });
});
