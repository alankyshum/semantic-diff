import { test, expect } from '../fixtures';
import AxeBuilder from '@axe-core/playwright';
import type { Page } from '@playwright/test';

// ---------------------------------------------------------------------------
// §4.2 — register console + pageerror listeners on every test in this file.
// ---------------------------------------------------------------------------
let consoleErrors: string[] = [];

test.beforeEach(async ({ page }) => {
  consoleErrors = [];
  page.on('console', (m) => {
    if (m.type() === 'error') consoleErrors.push(`[console.error] ${m.text()}`);
  });
  page.on('pageerror', (e) => {
    consoleErrors.push(`[pageerror] ${e.message}`);
  });
});

test.afterEach(async () => {
  expect(consoleErrors, `console errors observed:\n${consoleErrors.join('\n')}`).toEqual([]);
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Set theme preference via localStorage and reload, so the no-FOUC bootstrap
 * picks it up before hydration. Simpler & more deterministic than clicking
 * the toggle (which cycles auto -> light -> dark).
 */
async function setThemeAndReload(page: Page, theme: 'light' | 'dark') {
  await page.evaluate((t) => {
    localStorage.setItem('theme-pref', t);
  }, theme);
  await page.reload({ waitUntil: 'domcontentloaded' });
  // Confirm the bootstrap took effect.
  const applied = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
  expect(applied).toBe(theme);
}

async function runAxe(page: Page, label: string) {
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa']).analyze();
  if (results.violations.length > 0) {
    // Surface a compact diagnostic in the failure log.
    const summary = results.violations
      .map((v) => {
        const nodes = v.nodes
          .slice(0, 3)
          .map((n) => n.target.join(' '))
          .join(' | ');
        return `  - ${v.id} (${v.impact ?? 'n/a'}): ${nodes}`;
      })
      .join('\n');
    // eslint-disable-next-line no-console
    console.log(`AXE VIOLATIONS @ ${label}:\n${summary}`);
  }
  expect(results.violations, `axe violations at ${label}`).toEqual([]);
}

// ---------------------------------------------------------------------------
// §4.1 — Accessibility scans across routes × themes
// ---------------------------------------------------------------------------

const ROUTES: Array<{ name: string; path: (id: string) => string }> = [
  { name: 'index', path: () => '/' },
  { name: 'result-detail', path: (id) => `/r/${id}` },
  { name: 'result-issues', path: (id) => `/r/${id}/issues` },
  { name: 'settings', path: () => '/settings' },
];

const THEMES: Array<'light' | 'dark'> = ['light', 'dark'];

for (const route of ROUTES) {
  for (const theme of THEMES) {
    test(`a11y: ${route.name} — ${theme}`, async ({ page, baseURL, resultId }) => {
      const url = `${baseURL}${route.path(resultId)}`;
      // Pre-seed theme so the no-FOUC bootstrap applies the right `data-theme`
      // before any of the page's own scripts evaluate.
      await page.goto(url, { waitUntil: 'domcontentloaded' });
      await setThemeAndReload(page, theme);
      // Wait for the app to settle before scanning.
      await page.waitForLoadState('networkidle').catch(() => {
        /* networkidle may never resolve if SSE keep-alive is open; ignore. */
      });
      await runAxe(page, `${route.name} (${theme})`);
    });
  }
}

// ---------------------------------------------------------------------------
// §4.3 — CSP / inline-script discipline
// ---------------------------------------------------------------------------

test('CSP: only one inline <script> in <head> on /', async ({ page, baseURL }) => {
  await page.goto(`${baseURL}/`, { waitUntil: 'domcontentloaded' });
  const inlineCount = await page.locator('head > script:not([src])').count();
  expect(inlineCount).toBe(1);
});

// ---------------------------------------------------------------------------
// §4.4 — SSE smoke
// ---------------------------------------------------------------------------

test('SSE: /api/result/:id/events emits complete or closes cleanly within 3s', async ({
  page,
  baseURL,
  resultId,
}) => {
  // We need an `EventSource` available in a document context, so navigate to
  // the result page first and run the probe via page.evaluate.
  await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

  const outcome = await page.evaluate(
    ({ baseURL, resultId }) =>
      new Promise<{ status: string; events: string[]; closed: boolean }>((resolve) => {
        const events: string[] = [];
        let resolved = false;
        const finish = (status: string, closed: boolean) => {
          if (resolved) return;
          resolved = true;
          try {
            es.close();
          } catch {
            /* ignore */
          }
          resolve({ status, events, closed });
        };

        const es = new EventSource(`${baseURL}/api/result/${resultId}/events`);

        // Server emits all events as the named `section-updated` event,
        // with the final payload being `data: complete`. We accept either:
        //   (a) any message whose data is exactly "complete", or
        //   (b) the stream closing cleanly (readyState === CLOSED without error).
        const onMessageLike = (ev: MessageEvent) => {
          const data = String(ev.data ?? '');
          events.push(`${ev.type}:${data}`);
          if (data === 'complete') finish('complete-event', false);
        };

        es.addEventListener('section-updated', onMessageLike as EventListener);
        es.addEventListener('complete', onMessageLike as EventListener);
        es.onmessage = onMessageLike;

        es.onerror = () => {
          // EventSource fires `error` on close. If readyState===CLOSED and we
          // never saw an error during data flow, treat as a clean close.
          if (es.readyState === EventSource.CLOSED) {
            finish('closed', true);
          }
        };

        // Hard cap: 3 seconds.
        setTimeout(() => finish('timeout', es.readyState === EventSource.CLOSED), 3000);
      }),
    { baseURL, resultId },
  );

  expect(
    outcome.status === 'complete-event' || (outcome.status === 'closed' && outcome.closed),
    `SSE outcome=${outcome.status} closed=${outcome.closed} events=${JSON.stringify(outcome.events)}`,
  ).toBe(true);
});
