import { test, expect } from '../fixtures';

// Plan §3.2 — F4 Light/dark theme

test.describe('F4 — theme', () => {
  test('row 1+2: dark colorScheme applied at domcontentloaded (no FOUC)', async ({
    page,
    baseURL,
  }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto(`${baseURL}/`, { waitUntil: 'domcontentloaded' });
    const theme = await page.evaluate(() =>
      document.documentElement.getAttribute('data-theme'),
    );
    expect(theme).not.toBeNull();
    expect(theme).toBe('dark');
  });

  test('row 3: clicking theme toggle cycles auto → light → dark → auto', async ({
    page,
    baseURL,
  }) => {
    await page.goto(`${baseURL}/`, { waitUntil: 'domcontentloaded' });
    const toggle = page.locator('.theme-toggle').first();
    await expect(toggle).toBeVisible();

    const labelRe = /^Theme:\s+(auto|light|dark)\b/;

    const readState = async () => {
      const label = await toggle.getAttribute('aria-label');
      const dataTheme = await page.evaluate(() =>
        document.documentElement.getAttribute('data-theme'),
      );
      const pref = await page.evaluate(() => localStorage.getItem('theme-pref'));
      const m = label?.match(labelRe);
      return { mode: m?.[1] ?? null, label, dataTheme, pref };
    };

    const seen: string[] = [];
    const initial = await readState();
    expect(initial.label).toMatch(labelRe);
    seen.push(initial.mode!);

    for (let i = 0; i < 3; i++) {
      await toggle.click();
      const s = await readState();
      expect(s.label).toMatch(labelRe);
      // localStorage should be updated each click
      expect(s.pref).not.toBeNull();
      seen.push(s.mode!);
    }

    // Cycle of length 3 starting from current state — first and last entry equal.
    expect(seen[0]).toBe(seen[3]);
    // All three modes appear in the cycle.
    expect(new Set(seen).size).toBe(3);
    // Plan-specified order: auto → light → dark → auto. Find auto offset and verify.
    const order = ['auto', 'light', 'dark'];
    const start = order.indexOf(seen[0]);
    expect(start).toBeGreaterThanOrEqual(0);
    for (let i = 0; i < 4; i++) {
      expect(seen[i]).toBe(order[(start + i) % 3]);
    }
  });

  test('row 4: mermaid svg re-renders on theme toggle', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const mermaid = page.locator('figure.mermaid-container svg').first();
    await expect(mermaid).toBeVisible({ timeout: 10_000 });
    const before = await mermaid.evaluate((el) => el.outerHTML);
    await page.locator('.theme-toggle').first().click();
    // Wait for re-render
    await page.waitForTimeout(500);
    const after = await page
      .locator('figure.mermaid-container svg')
      .first()
      .evaluate((el) => el.outerHTML);
    expect(after).not.toBe(before);
  });

  // eslint-disable-next-line playwright/no-skipped-test
  test.skip('row 5: axe scan on / and /r/:id in light + dark — moved to cross-cutting suite', () => {
    // Cross-cutting axe scan to be added in a separate file (see plan §4.1).
  });
});
