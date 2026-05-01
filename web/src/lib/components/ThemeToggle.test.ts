import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { render, fireEvent } from '@testing-library/svelte';
import ThemeToggle from './ThemeToggle.svelte';
import { themePref } from '$lib/stores/theme';

beforeEach(() => {
  localStorage.clear();
  themePref.set('auto');
});

describe('ThemeToggle', () => {
  it('cycles the pref store on click', async () => {
    const { getByRole } = render(ThemeToggle);
    const btn = getByRole('button');

    expect(get(themePref)).toBe('auto');
    await fireEvent.click(btn);
    expect(get(themePref)).toBe('light');
    await fireEvent.click(btn);
    expect(get(themePref)).toBe('dark');
    await fireEvent.click(btn);
    expect(get(themePref)).toBe('auto');
  });

  it('aria-label reflects current theme pref', async () => {
    const { getByRole } = render(ThemeToggle);
    const btn = getByRole('button');
    expect(btn.getAttribute('aria-label')).toMatch(/auto/i);
    await fireEvent.click(btn);
    expect(btn.getAttribute('aria-label')).toMatch(/light/i);
  });
});
