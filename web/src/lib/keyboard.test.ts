import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import {
  register,
  dispatch,
  registry,
  __test,
  __resetSequenceState,
  SEQUENCE_TIMEOUT_MS,
  type Shortcut,
} from './keyboard';
import { get } from 'svelte/store';

function ev(key: string, mods: Partial<{ meta: boolean; ctrl: boolean; shift: boolean; alt: boolean }> = {}, target?: EventTarget): KeyboardEvent {
  const e = new KeyboardEvent('keydown', {
    key,
    metaKey: !!mods.meta,
    ctrlKey: !!mods.ctrl,
    shiftKey: !!mods.shift,
    altKey: !!mods.alt,
    bubbles: true,
    cancelable: true,
  });
  if (target) {
    Object.defineProperty(e, 'target', { value: target, configurable: true });
  }
  return e;
}

beforeEach(() => {
  registry.set([]);
  __resetSequenceState();
});

describe('keyboard combo parser', () => {
  it('parses simple key', () => {
    const p = __test.parseCombo('j', true);
    expect(p.atoms).toHaveLength(1);
    expect(p.atoms[0].key).toBe('j');
    expect(p.atoms[0].meta).toBe(false);
  });

  it('maps cmd to metaKey on macOS', () => {
    const p = __test.parseCombo('cmd+k', true);
    expect(p.atoms[0].key).toBe('k');
    expect(p.atoms[0].meta).toBe(true);
    expect(p.atoms[0].ctrl).toBe(false);
  });

  it('maps cmd to ctrlKey on non-mac', () => {
    const p = __test.parseCombo('cmd+k', false);
    expect(p.atoms[0].meta).toBe(false);
    expect(p.atoms[0].ctrl).toBe(true);
  });

  it('parses sequence', () => {
    const p = __test.parseCombo('g i', true);
    expect(p.atoms).toHaveLength(2);
    expect(p.atoms[0].key).toBe('g');
    expect(p.atoms[1].key).toBe('i');
  });

  it('parses multi-modifier combo', () => {
    const p = __test.parseCombo('ctrl+shift+x', false);
    expect(p.atoms[0]).toMatchObject({ key: 'x', ctrl: true, shift: true });
  });
});

describe('dispatch — single combos', () => {
  it('fires matching single-key shortcut and returns true', () => {
    const handler = vi.fn();
    register({ combo: 'j', scope: 'review-detail', label: 'next', handler });
    const fired = dispatch(ev('j'), 'review-detail');
    expect(fired).toBe(true);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('returns false when no match', () => {
    const handler = vi.fn();
    register({ combo: 'j', scope: 'review-detail', label: 'next', handler });
    const fired = dispatch(ev('x'), 'review-detail');
    expect(fired).toBe(false);
    expect(handler).not.toHaveBeenCalled();
  });

  it('does not fire when modifiers differ', () => {
    const handler = vi.fn();
    register({ combo: 'j', scope: 'review-detail', label: 'x', handler });
    const fired = dispatch(ev('j', { meta: true }), 'review-detail');
    expect(fired).toBe(false);
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('dispatch — scope filtering', () => {
  it('only dispatches matching-scope or global shortcuts', () => {
    const detail = vi.fn();
    const index = vi.fn();
    const global = vi.fn();
    register({ combo: 'a', scope: 'review-detail', label: '', handler: detail });
    register({ combo: 'b', scope: 'index', label: '', handler: index });
    register({ combo: 'c', scope: 'global', label: '', handler: global });

    expect(dispatch(ev('a'), 'index')).toBe(false);
    expect(dispatch(ev('b'), 'index')).toBe(true);
    expect(dispatch(ev('c'), 'index')).toBe(true);
    expect(detail).not.toHaveBeenCalled();
    expect(index).toHaveBeenCalled();
    expect(global).toHaveBeenCalled();
  });

  it('palette scope suppresses all non-palette shortcuts', () => {
    const reviewH = vi.fn();
    const globalH = vi.fn();
    const paletteH = vi.fn();
    register({ combo: 'a', scope: 'review-detail', label: '', handler: reviewH });
    register({ combo: 'b', scope: 'global', label: '', handler: globalH });
    register({ combo: 'escape', scope: 'palette', label: '', handler: paletteH });

    expect(dispatch(ev('a'), 'palette')).toBe(false);
    expect(dispatch(ev('b'), 'palette')).toBe(false);
    expect(dispatch(ev('Escape'), 'palette')).toBe(true);
    expect(reviewH).not.toHaveBeenCalled();
    expect(globalH).not.toHaveBeenCalled();
    expect(paletteH).toHaveBeenCalled();
  });
});

describe('dispatch — sequence handling', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('fires g i within timeout', () => {
    const handler = vi.fn();
    register({ combo: 'g i', scope: 'global', label: '', handler });
    expect(dispatch(ev('g'), 'global')).toBe(false);
    expect(handler).not.toHaveBeenCalled();
    expect(dispatch(ev('i'), 'global')).toBe(true);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('drops sequence after timeout', () => {
    const handler = vi.fn();
    register({ combo: 'g i', scope: 'global', label: '', handler });
    dispatch(ev('g'), 'global');
    vi.advanceTimersByTime(SEQUENCE_TIMEOUT_MS + 10);
    expect(dispatch(ev('i'), 'global')).toBe(false);
    expect(handler).not.toHaveBeenCalled();
  });

  it('wrong second key resets and does not fire', () => {
    const handler = vi.fn();
    register({ combo: 'g i', scope: 'global', label: '', handler });
    dispatch(ev('g'), 'global');
    expect(dispatch(ev('z'), 'global')).toBe(false);
    expect(handler).not.toHaveBeenCalled();
    // Subsequent 'i' alone also doesn't fire (buffer cleared).
    expect(dispatch(ev('i'), 'global')).toBe(false);
  });
});

describe('dispatch — input focus skip', () => {
  it('skips when target is INPUT', () => {
    const handler = vi.fn();
    register({ combo: 'j', scope: 'review-detail', label: '', handler });
    const input = document.createElement('input');
    document.body.appendChild(input);
    const fired = dispatch(ev('j', {}, input), 'review-detail');
    expect(fired).toBe(false);
    expect(handler).not.toHaveBeenCalled();
    document.body.removeChild(input);
  });

  it('skips when target is TEXTAREA', () => {
    const handler = vi.fn();
    register({ combo: 'k', scope: 'review-detail', label: '', handler });
    const ta = document.createElement('textarea');
    document.body.appendChild(ta);
    expect(dispatch(ev('k', {}, ta), 'review-detail')).toBe(false);
    expect(handler).not.toHaveBeenCalled();
    document.body.removeChild(ta);
  });

  it('still fires Escape inside an input', () => {
    const handler = vi.fn();
    register({ combo: 'escape', scope: 'global', label: '', handler });
    const input = document.createElement('input');
    document.body.appendChild(input);
    expect(dispatch(ev('Escape', {}, input), 'global')).toBe(true);
    expect(handler).toHaveBeenCalled();
    document.body.removeChild(input);
  });

  it('still fires cmd+k inside an input', () => {
    const handler = vi.fn();
    const mac = __test.isMac();
    register({ combo: 'cmd+k', scope: 'global', label: '', handler });
    const input = document.createElement('input');
    document.body.appendChild(input);
    const fired = dispatch(ev('k', mac ? { meta: true } : { ctrl: true }, input), 'global');
    expect(fired).toBe(true);
    expect(handler).toHaveBeenCalled();
    document.body.removeChild(input);
  });
});

describe('register', () => {
  it('returns an unregister function', () => {
    const handler = vi.fn();
    const off = register({ combo: 'q', scope: 'global', label: '', handler });
    expect(get(registry)).toHaveLength(1);
    off();
    expect(get(registry)).toHaveLength(0);
  });
});

// Force test discovery for type Shortcut.
const _typeProbe: Shortcut = { combo: 'x', scope: 'global', label: '', handler: () => {} };
void _typeProbe;
