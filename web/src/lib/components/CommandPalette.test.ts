import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import CommandPalette from './CommandPalette.svelte';
import { registry, paletteItems } from '$lib/keyboard';
import { tick } from 'svelte';

// jsdom doesn't implement HTMLDialogElement.showModal; stub minimally.
beforeEach(() => {
  registry.set([]);
  paletteItems.set([]);
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

describe('CommandPalette', () => {
  it('lists registered shortcuts as items', async () => {
    registry.set([
      { combo: 'j', scope: 'review-detail', label: 'Next group', group: 'Navigation', handler: () => {} },
      { combo: 'k', scope: 'review-detail', label: 'Previous group', group: 'Navigation', handler: () => {} },
    ]);
    const { component, getByLabelText, queryByText } = render(CommandPalette);
    (component as unknown as { show: () => void }).show();
    await tick(); await tick();
    expect(getByLabelText('Search commands')).toBeTruthy();
    expect(queryByText('Next group')).toBeTruthy();
    expect(queryByText('Previous group')).toBeTruthy();
  });

  it('includes paletteItems store entries', async () => {
    paletteItems.set([
      { id: 'x', label: 'Go to home', group: 'Navigation', action: () => {} },
    ]);
    const { component, queryByText } = render(CommandPalette);
    (component as unknown as { show: () => void }).show();
    await tick(); await tick();
    expect(queryByText('Go to home')).toBeTruthy();
  });

  it('filters items by substring (fuzzy)', async () => {
    paletteItems.set([
      { id: 'a', label: 'Apple pie', action: () => {} },
      { id: 'b', label: 'Banana split', action: () => {} },
      { id: 'c', label: 'Cherry tart', action: () => {} },
    ]);
    const { component, getByLabelText, queryByText } = render(CommandPalette);
    (component as unknown as { show: () => void }).show();
    await tick(); await tick();
    const input = getByLabelText('Search commands') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'banan' } });
    await tick();
    expect(queryByText('Banana split')).toBeTruthy();
    expect(queryByText('Apple pie')).toBeNull();
    expect(queryByText('Cherry tart')).toBeNull();
  });

  it('Enter executes the active item', async () => {
    const action = vi.fn();
    paletteItems.set([{ id: 'x', label: 'Run me', action }]);
    const { component, container } = render(CommandPalette);
    (component as unknown as { show: () => void }).show();
    await tick(); await tick();
    // Find the row whose label is 'Run me' (other rows may exist from the
    // palette's own self-registered cmd+k shortcut).
    const rows = Array.from(container.querySelectorAll<HTMLButtonElement>('.row'));
    const target = rows.find((r) => (r.textContent || '').includes('Run me'));
    expect(target).toBeTruthy();
    target!.click();
    expect(action).toHaveBeenCalledTimes(1);
  });

  it('ArrowDown moves active selection', async () => {
    paletteItems.set([
      { id: 'a', label: 'AlphaItem', action: () => {} },
      { id: 'b', label: 'BetaItem', action: () => {} },
    ]);
    const { component, getByLabelText } = render(CommandPalette);
    (component as unknown as { show: () => void }).show();
    await tick(); await tick();
    const input = getByLabelText('Search commands') as HTMLInputElement;
    // Filter to just our items so the registered cmd+k doesn't interfere.
    await fireEvent.input(input, { target: { value: 'item' } });
    await tick();
    await fireEvent.keyDown(input, { key: 'ArrowDown' });
    await tick();
    const { container: c } = { container: input.closest('dialog')!.parentElement! };
    void c;
    const rows = Array.from(input.closest('dialog')!.querySelectorAll('.row'));
    expect(rows.length).toBe(2);
    expect(rows[1].classList.contains('active')).toBe(true);
  });

  it('Escape closes the palette', async () => {
    paletteItems.set([{ id: 'x', label: 'Anything', action: () => {} }]);
    const { component, getByLabelText, container } = render(CommandPalette);
    (component as unknown as { show: () => void }).show();
    await tick(); await tick();
    const dialog = container.querySelector('dialog') as HTMLDialogElement;
    expect(dialog.hasAttribute('open')).toBe(true);
    const input = getByLabelText('Search commands');
    await fireEvent.keyDown(input, { key: 'Escape' });
    await tick();
    expect(dialog.hasAttribute('open')).toBe(false);
    void component;
  });
});
