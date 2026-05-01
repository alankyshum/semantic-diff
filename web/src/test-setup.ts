// Vitest global setup — runs before test modules are imported.
// Stubs window.matchMedia so the theme store can read it on first import.

if (typeof window !== 'undefined' && !window.matchMedia) {
  (window as Window & { matchMedia: (q: string) => MediaQueryList }).matchMedia = (query: string) => ({
    matches: query.includes('dark'),
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => true,
  } as unknown as MediaQueryList);
}
