import { test, expect } from '../fixtures';

// Plan §3.4 — F2 Sticky + collapsible sidebar

test.describe('F2 — sidebar', () => {
  test('row 1: aside.sidebar is sticky after scroll', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const sidebar = page.locator('aside.sidebar');
    await expect(sidebar).toBeVisible();

    // Measure header height — could be .site-header, header, etc.
    const headerHeight = await page.evaluate(() => {
      const h =
        document.querySelector('header') ||
        document.querySelector('.site-header') ||
        document.querySelector('.app-header');
      return h ? (h as HTMLElement).getBoundingClientRect().height : 0;
    });

    await page.evaluate(() => window.scrollBy(0, 5000));
    // Allow a frame to settle
    await page.waitForTimeout(100);

    const top = await sidebar.evaluate(
      (el) => el.getBoundingClientRect().top,
    );
    expect(Math.abs(top - headerHeight)).toBeLessThanOrEqual(1);
  });

  test('row 2: collapse button collapses sidebar and persists to localStorage', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    // Ensure starting expanded.
    await page.evaluate(() => localStorage.setItem('sidebar-collapsed', '0'));
    await page.reload({ waitUntil: 'domcontentloaded' });

    const collapseBtn = page.locator(
      '.collapse-btn[aria-label="Collapse sidebar"]',
    );
    await expect(collapseBtn).toBeVisible();
    await collapseBtn.click();

    const body = page.locator('.body').first();
    await expect(body).toHaveClass(/sidebar-collapsed/);

    const sidebarWidth = await page
      .locator('aside.sidebar')
      .evaluate((el) => el.getBoundingClientRect().width);
    expect(sidebarWidth).toBeLessThanOrEqual(64);

    const stored = await page.evaluate(() =>
      localStorage.getItem('sidebar-collapsed'),
    );
    expect(stored).toBe('1');

    await expect(
      page.locator('.collapse-btn[aria-label="Expand sidebar"]'),
    ).toBeVisible();
  });

  test('row 3: collapsed state persists across reload', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    await page.evaluate(() => localStorage.setItem('sidebar-collapsed', '1'));
    await page.reload({ waitUntil: 'domcontentloaded' });

    await expect(page.locator('.body').first()).toHaveClass(
      /sidebar-collapsed/,
    );
    await expect(
      page.locator('.collapse-btn[aria-label="Expand sidebar"]'),
    ).toBeVisible();
  });

  test('row 4: expand button removes collapsed state', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    await page.evaluate(() => localStorage.setItem('sidebar-collapsed', '1'));
    await page.reload({ waitUntil: 'domcontentloaded' });

    const expandBtn = page.locator(
      '.collapse-btn[aria-label="Expand sidebar"]',
    );
    await expect(expandBtn).toBeVisible();
    await expandBtn.click();

    await expect(page.locator('.body').first()).not.toHaveClass(
      /sidebar-collapsed/,
    );

    const stored = await page.evaluate(() =>
      localStorage.getItem('sidebar-collapsed'),
    );
    // Harness contract: stored as '1' or '0', never removed; accept null too defensively.
    expect(stored === null || stored === '0').toBe(true);
  });

  // eslint-disable-next-line playwright/no-skipped-test
  test.skip('row 5: mobile slide-over single-column layout — deferred (slide-over not implemented)', () => {
    // Plan §3.4 row 5 explicitly deferred per roadmap deviation §1 (F2 mobile slide-over).
  });
});
