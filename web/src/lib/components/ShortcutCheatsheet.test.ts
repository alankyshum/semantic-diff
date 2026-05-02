import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import ShortcutCheatsheet from './ShortcutCheatsheet.svelte';
import { registry } from '$lib/keyboard';
import { tick } from 'svelte';

beforeEach(() => {
  registry.set([]);
  if (!HTMLDialogElement.prototype.showModal) {
    HTMLDialogElement.prototype.showModal = function () {
      this.setAttribute('open', '');
      (this as unknown as { open: boolean }).open = true;
    };
  }
  if (!HTMLDialogElement.prototype.close) {
    HTMLDialogElement.prototype.close = function () {
      this.removeAttribute('open');
      (this as unknown as { open: boolean }).open = false;
    };
  }
});

describe('ShortcutCheatsheet', () => {
  it('renders all registered shortcuts grouped by group', async () => {
    registry.set([
      { combo: 'j', scope: 'review-detail', label: 'Next group', group: 'Navigation', handler: () => {} },
      { combo: 'k', scope: 'review-detail', label: 'Previous group', group: 'Navigation', handler: () => {} },
      { combo: 'v', scope: 'review-detail', label: 'Cycle view', group: 'View', handler: () => {} },
    ]);
    const { component, container, queryByText } = render(ShortcutCheatsheet);
    (component as unknown as { open_: () => void }).open_();
    await tick(); await tick();

    // Group headers
    expect(queryByText('Navigation')).toBeTruthy();
    expect(queryByText('View')).toBeTruthy();
    // Labels
    expect(queryByText('Next group')).toBeTruthy();
    expect(queryByText('Previous group')).toBeTruthy();
    expect(queryByText('Cycle view')).toBeTruthy();
    // Combos rendered as <kbd>
    const kbds = Array.from(container.querySelectorAll('kbd')).map((k) => k.textContent);
    expect(kbds).toContain('j');
    expect(kbds).toContain('k');
    expect(kbds).toContain('v');
  });

  it('omits palette-scoped and unlabeled shortcuts', async () => {
    registry.set([
      { combo: 'escape', scope: 'palette', label: 'Close palette', handler: () => {} },
      { combo: 'x', scope: 'global', label: '', handler: () => {} },
      { combo: 'y', scope: 'global', label: 'Visible', group: 'General', handler: () => {} },
    ]);
    const { component, queryByText } = render(ShortcutCheatsheet);
    (component as unknown as { open_: () => void }).open_();
    await tick(); await tick();
    expect(queryByText('Visible')).toBeTruthy();
    expect(queryByText('Close palette')).toBeNull();
  });

  it('updates when registry changes (auto-reactive)', async () => {
    registry.set([]);
    const { component, queryByText } = render(ShortcutCheatsheet);
    (component as unknown as { open_: () => void }).open_();
    await tick(); await tick();
    expect(queryByText('Late shortcut')).toBeNull();
    registry.set([
      { combo: 'q', scope: 'global', label: 'Late shortcut', group: 'General', handler: () => {} },
    ]);
    await tick();
    expect(queryByText('Late shortcut')).toBeTruthy();
  });
});
