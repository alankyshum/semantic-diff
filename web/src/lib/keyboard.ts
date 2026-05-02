// F17 — Keyboard shortcut registry + dispatcher.
//
// Exposes a small registry of `Shortcut`s that pages can register/unregister
// in their lifecycle hooks, plus a `dispatch()` helper called from a single
// document-level keydown listener installed in `+layout.svelte`.
//
// Combo grammar:
//   - Modifiers joined by `+`: `cmd+k`, `shift+/`, `ctrl+alt+x`.
//     `cmd` maps to metaKey on macOS and ctrlKey elsewhere.
//   - Sequences (g-prefix style) joined by a single space: `g i`, `g h`.
//     The first key is buffered for `SEQUENCE_TIMEOUT_MS`; if the second
//     key arrives in time, the full sequence is matched.
//
// Scope semantics:
//   - 'global' shortcuts always considered.
//   - Page-specific scopes ('index', 'review-detail') are only active when
//     the layout has set that scope.
//   - 'palette' is a transient scope: when active, ONLY palette-scoped
//     shortcuts dispatch (everything else is suppressed).

import { writable } from 'svelte/store';

export type Scope = 'global' | 'review-detail' | 'index' | 'palette';

export interface Shortcut {
  combo: string;
  scope: Scope;
  label: string;
  group?: string;
  // The event is provided when invoked from a real keydown; it may be omitted
  // when invoked synthetically (e.g. from the command palette). Handlers
  // should not rely on it being present.
  handler: (e?: KeyboardEvent) => void;
}

export interface PaletteItem {
  id: string;
  label: string;
  group?: string;
  combo?: string;
  action: () => void;
}

export const registry = writable<Shortcut[]>([]);
export const paletteItems = writable<PaletteItem[]>([]);
export const currentScope = writable<Scope>('global');

export const SEQUENCE_TIMEOUT_MS = 1500;

// --- Platform detection ----------------------------------------------------
function isMac(): boolean {
  if (typeof navigator === 'undefined') return false;
  // navigator.platform is deprecated but still reliable for this check;
  // userAgent fallback covers modern browsers.
  const p = (navigator.platform || '').toLowerCase();
  if (p.includes('mac')) return true;
  return /mac|iphone|ipad|ipod/i.test(navigator.userAgent || '');
}

// --- Combo parsing ---------------------------------------------------------
interface ParsedAtom {
  key: string;        // lowercase non-modifier key (e.g. 'k', '/', 'arrowdown')
  meta: boolean;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
}

interface ParsedCombo {
  atoms: ParsedAtom[];   // length 1 for single, >1 for sequence
}

function parseAtom(raw: string, mac: boolean): ParsedAtom {
  const parts = raw.split('+').map(s => s.trim().toLowerCase()).filter(Boolean);
  const atom: ParsedAtom = { key: '', meta: false, ctrl: false, shift: false, alt: false };
  for (const p of parts) {
    if (p === 'cmd') {
      if (mac) atom.meta = true; else atom.ctrl = true;
    } else if (p === 'meta') {
      atom.meta = true;
    } else if (p === 'ctrl' || p === 'control') {
      atom.ctrl = true;
    } else if (p === 'shift') {
      atom.shift = true;
    } else if (p === 'alt' || p === 'option') {
      atom.alt = true;
    } else {
      atom.key = p;
    }
  }
  return atom;
}

function parseCombo(combo: string, mac: boolean): ParsedCombo {
  // Sequence atoms separated by single spaces. We split on spaces NOT inside
  // a `+` group — but our grammar disallows spaces in atoms, so a plain split
  // is safe.
  const atomStrs = combo.split(/\s+/).filter(Boolean);
  return { atoms: atomStrs.map(s => parseAtom(s, mac)) };
}

function eventToAtom(e: KeyboardEvent): ParsedAtom {
  return {
    key: (e.key || '').toLowerCase(),
    meta: !!e.metaKey,
    ctrl: !!e.ctrlKey,
    shift: !!e.shiftKey,
    alt: !!e.altKey,
  };
}

/**
 * Match a parsed combo atom against an observed keyboard event atom.
 *
 * Shift handling: combos that include a punctuation key requiring shift on US
 * keyboards (e.g. `?`, `[`, `]`) match regardless of the shift modifier state,
 * since these keys produce different characters on different layouts and the
 * resolved `e.key` is already the post-shift character. For combos that
 * explicitly require shift (e.g. `shift+/`), use the `shift+` prefix in the
 * combo string — that path enforces shift via `parsed.shift`.
 */
function atomMatches(parsed: ParsedAtom, ev: ParsedAtom): boolean {
  if (parsed.key !== ev.key) return false;
  if (parsed.meta !== ev.meta) return false;
  if (parsed.ctrl !== ev.ctrl) return false;
  if (parsed.alt !== ev.alt) return false;
  // For shift: if the parsed combo specifies shift OR the key itself is a
  // letter we want exact match. For symbol keys like '?' which are produced
  // via shift, the e.key is already '?' and we don't require shift to be set
  // in the parsed atom — we tolerate shift being either on or off when not
  // explicitly required. To keep things deterministic we only ENFORCE shift
  // when the parsed atom requested it.
  if (parsed.shift && !ev.shift) return false;
  return true;
}

// Exported for tests.
export const __test = { parseCombo, parseAtom, atomMatches, eventToAtom, isMac };

// --- Registration ----------------------------------------------------------
let _shortcuts: Shortcut[] = [];
registry.subscribe((s) => { _shortcuts = s; });

export function register(shortcut: Shortcut): () => void {
  registry.update((list) => [...list, shortcut]);
  return () => {
    registry.update((list) => list.filter((s) => s !== shortcut));
  };
}

export function activeScope(scope: Scope): () => void {
  const prev = _readScope();
  currentScope.set(scope);
  return () => {
    // Only revert if we're still the active scope; if someone else changed it
    // already, leave it alone.
    if (_readScope() === scope) currentScope.set(prev);
  };
}

let _scopeValue: Scope = 'global';
currentScope.subscribe((v) => { _scopeValue = v; });
function _readScope(): Scope { return _scopeValue; }

// --- Input-focus skip ------------------------------------------------------
function isEditableTarget(t: EventTarget | null): boolean {
  if (typeof document === 'undefined') return false;
  const el = (t as Element | null) ?? document.activeElement;
  if (!el || !(el as HTMLElement).tagName) return false;
  const tag = (el as HTMLElement).tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if ((el as HTMLElement).isContentEditable) return true;
  return false;
}

// Always-allowed combos even when an input has focus.
function isAlwaysAllowed(combo: string): boolean {
  const c = combo.toLowerCase();
  return c === 'escape' || c === 'cmd+k' || c === 'meta+k' || c === 'ctrl+k';
}

// --- Sequence buffer -------------------------------------------------------
let _seqAtom: ParsedAtom | null = null;
let _seqTimer: ReturnType<typeof setTimeout> | null = null;

function clearSeq() {
  _seqAtom = null;
  if (_seqTimer) {
    clearTimeout(_seqTimer);
    _seqTimer = null;
  }
}

function startSeq(atom: ParsedAtom) {
  _seqAtom = atom;
  if (_seqTimer) clearTimeout(_seqTimer);
  _seqTimer = setTimeout(() => { clearSeq(); }, SEQUENCE_TIMEOUT_MS);
}

// Test-only helper to reset internal sequence state between cases.
export function __resetSequenceState(): void { clearSeq(); }

// --- Dispatch --------------------------------------------------------------
const _mac = isMac();

function shortcutsForScope(scope: Scope): Shortcut[] {
  if (scope === 'palette') {
    return _shortcuts.filter((s) => s.scope === 'palette');
  }
  return _shortcuts.filter((s) => s.scope === 'global' || s.scope === scope);
}

export function dispatch(e: KeyboardEvent, scope: Scope): boolean {
  const ev = eventToAtom(e);

  // Ignore lone modifier keypresses (key === 'shift'/'control'/etc).
  if (ev.key === 'shift' || ev.key === 'control' || ev.key === 'alt' || ev.key === 'meta') {
    return false;
  }

  const candidates = shortcutsForScope(scope);
  const editable = isEditableTarget(e.target);

  // Try sequence first if a prefix is buffered.
  if (_seqAtom) {
    const prefix = _seqAtom;
    // Reset buffer regardless of match result.
    clearSeq();
    for (const sc of candidates) {
      const parsed = parseCombo(sc.combo, _mac);
      if (parsed.atoms.length !== 2) continue;
      if (!atomMatches(parsed.atoms[0], prefix)) continue;
      if (!atomMatches(parsed.atoms[1], ev)) continue;
      if (editable && !isAlwaysAllowed(sc.combo)) continue;
      sc.handler(e);
      return true;
    }
    // Fall through: this key wasn't a valid completion; treat it as a fresh key.
  }

  // Single-atom direct match.
  for (const sc of candidates) {
    const parsed = parseCombo(sc.combo, _mac);
    if (parsed.atoms.length !== 1) continue;
    if (!atomMatches(parsed.atoms[0], ev)) continue;
    if (editable && !isAlwaysAllowed(sc.combo)) continue;
    sc.handler(e);
    return true;
  }

  // No single match — could this start a sequence? Only buffer when not editable.
  if (!editable) {
    for (const sc of candidates) {
      const parsed = parseCombo(sc.combo, _mac);
      if (parsed.atoms.length !== 2) continue;
      if (atomMatches(parsed.atoms[0], ev)) {
        startSeq(ev);
        return false; // prefix consumed but no shortcut fired yet
      }
    }
  }

  return false;
}
