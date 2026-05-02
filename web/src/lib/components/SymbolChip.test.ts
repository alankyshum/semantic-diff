import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import SymbolChip from './SymbolChip.svelte';

describe('SymbolChip', () => {
  it('renders basename only when no line and no repoUrl', () => {
    const { container } = render(SymbolChip, { props: { file: 'src/lib/foo/bar.ts' } });
    const chip = container.querySelector('.symbol-chip') as HTMLElement;
    expect(chip).toBeTruthy();
    expect(chip.tagName).toBe('SPAN');
    expect(chip.textContent).toBe('bar.ts');
    expect(chip.getAttribute('title')).toBe('src/lib/foo/bar.ts');
  });

  it('renders basename:line when line given but no repoUrl', () => {
    const { container } = render(SymbolChip, {
      props: { file: 'src/lib/foo/bar.ts', line: 42 },
    });
    const chip = container.querySelector('.symbol-chip') as HTMLElement;
    expect(chip.tagName).toBe('SPAN');
    expect(chip.textContent).toBe('bar.ts:42');
    expect(chip.getAttribute('title')).toBe('src/lib/foo/bar.ts:42');
  });

  it('renders <a> with proper href when repoUrl provided', () => {
    const { container } = render(SymbolChip, {
      props: {
        file: 'src/lib/foo/bar.ts',
        line: 42,
        repoUrl: 'https://github.com/acme/widget',
      },
    });
    const a = container.querySelector('a.symbol-chip') as HTMLAnchorElement;
    expect(a).toBeTruthy();
    expect(a.getAttribute('href')).toBe(
      'https://github.com/acme/widget/blob/HEAD/src/lib/foo/bar.ts#L42'
    );
    expect(a.getAttribute('target')).toBe('_blank');
    expect(a.getAttribute('rel')).toBe('noopener noreferrer');
    expect(a.textContent).toBe('bar.ts:42');
  });

  it('strips trailing .git from repoUrl in href', () => {
    const { container } = render(SymbolChip, {
      props: {
        file: 'src/lib/foo/bar.ts',
        repoUrl: 'https://github.com/acme/widget.git',
      },
    });
    const a = container.querySelector('a.symbol-chip') as HTMLAnchorElement;
    expect(a.getAttribute('href')).toBe(
      'https://github.com/acme/widget/blob/HEAD/src/lib/foo/bar.ts'
    );
  });
});
