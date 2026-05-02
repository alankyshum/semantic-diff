import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import SeverityBadge from './SeverityBadge.svelte';
import type { Severity } from '$lib/types';

const expectations: Array<{ sev: Severity; cssVar: string }> = [
  { sev: 'critical', cssVar: 'var(--color-danger)' },
  { sev: 'high', cssVar: 'var(--color-danger)' },
  { sev: 'medium', cssVar: 'var(--color-warning)' },
  { sev: 'low', cssVar: 'var(--color-info, var(--color-accent))' },
  { sev: 'nit', cssVar: 'var(--color-fg-muted)' },
  { sev: 'info', cssVar: 'var(--color-fg-muted)' },
];

describe('SeverityBadge', () => {
  for (const { sev, cssVar } of expectations) {
    it(`renders ${sev} with correct color`, () => {
      const { container } = render(SeverityBadge, { props: { severity: sev } });
      const badge = container.querySelector('.badge') as HTMLElement;
      expect(badge).toBeTruthy();
      expect(badge.classList.contains(`severity-${sev}`)).toBe(true);
      // inline style contains the css var token
      expect(badge.getAttribute('style')).toContain(cssVar);
    });
  }

  it('marks critical with critical class for stronger weight', () => {
    const { container } = render(SeverityBadge, { props: { severity: 'critical' } });
    const badge = container.querySelector('.badge') as HTMLElement;
    expect(badge.classList.contains('critical')).toBe(true);
  });

  it('falls back gracefully for legacy "none"', () => {
    const { container } = render(SeverityBadge, { props: { severity: 'none' } });
    const badge = container.querySelector('.badge') as HTMLElement;
    expect(badge.getAttribute('style')).toContain('var(--color-fg-muted)');
  });
});
