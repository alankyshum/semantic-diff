import { writable, derived, get, type Readable } from 'svelte/store';

export type ThemePref = 'auto' | 'light' | 'dark';
export type EffectiveTheme = 'light' | 'dark';

const STORAGE_KEY = 'theme-pref';
const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';

function readInitialPref(): ThemePref {
  if (!isBrowser) return 'auto';
  try {
    const v = window.localStorage.getItem(STORAGE_KEY);
    if (v === 'auto' || v === 'light' || v === 'dark') return v;
  } catch {
    /* localStorage may throw in private mode */
  }
  return 'auto';
}

/** User's preference. Persisted to localStorage on change. */
export const themePref = writable<ThemePref>(readInitialPref());

themePref.subscribe((v) => {
  if (!isBrowser) return;
  try {
    window.localStorage.setItem(STORAGE_KEY, v);
  } catch {
    /* ignore */
  }
});

/** System dark-mode preference, kept in sync with matchMedia. */
const systemDark = writable<boolean>(
  isBrowser && window.matchMedia
    ? window.matchMedia('(prefers-color-scheme: dark)').matches
    : true
);

let mediaListenerAttached = false;
function ensureMediaListener(): void {
  if (mediaListenerAttached) return;
  if (!isBrowser || !window.matchMedia) return;
  mediaListenerAttached = true;
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  const onChange = (e: MediaQueryListEvent) => systemDark.set(e.matches);
  // addEventListener is the modern API; fall back to addListener for old jsdom.
  if (typeof mq.addEventListener === 'function') {
    mq.addEventListener('change', onChange);
  } else if (typeof (mq as MediaQueryList & { addListener?: (cb: (e: MediaQueryListEvent) => void) => void }).addListener === 'function') {
    (mq as MediaQueryList & { addListener: (cb: (e: MediaQueryListEvent) => void) => void }).addListener(onChange);
  }
}
ensureMediaListener();

/** Effective theme = pref unless 'auto', in which case follow system. */
export const effectiveTheme: Readable<EffectiveTheme> = derived(
  [themePref, systemDark],
  ([$pref, $sysDark]) => {
    if ($pref === 'light') return 'light';
    if ($pref === 'dark') return 'dark';
    return $sysDark ? 'dark' : 'light';
  }
);

/** Apply effective theme to <html data-theme> and re-init mermaid. */
export function applyTheme(theme: EffectiveTheme): void {
  if (!isBrowser) return;
  ensureMediaListener();
  document.documentElement.dataset.theme = theme;
  // Mermaid theme — best-effort, never throw.
  import('mermaid')
    .then(({ default: mermaid }) => {
      mermaid.initialize({
        startOnLoad: false,
        theme: theme === 'dark' ? 'dark' : 'default',
        securityLevel: 'loose',
      });
    })
    .catch(() => {});
}

/** Cycle auto → light → dark → auto. */
export function cycleTheme(): void {
  const order: ThemePref[] = ['auto', 'light', 'dark'];
  const current = get(themePref);
  const next = order[(order.indexOf(current) + 1) % order.length];
  themePref.set(next);
}

/** Test-only helper to reset internal state. */
export function __resetForTests(initialPref: ThemePref = 'auto', sysDark = true): void {
  themePref.set(initialPref);
  systemDark.set(sysDark);
}
