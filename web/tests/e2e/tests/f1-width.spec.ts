import { test, expect } from '../fixtures';

// Plan §3.3 — F1 Responsive width

const VIEWPORTS = [
  { w: 375, h: 800 },
  { w: 768, h: 1024 },
  { w: 1024, h: 768 },
  { w: 1440, h: 900 },
  { w: 1920, h: 1080 },
];

const parsePx = (s: string | null | undefined): number => {
  if (!s) return NaN;
  const m = s.match(/([\d.]+)px/);
  return m ? parseFloat(m[1]) : NaN;
};

test.describe('F1 — responsive width', () => {
  for (const { w, h } of VIEWPORTS) {
    test(`row 1: no horizontal scrollbar at ${w}x${h}`, async ({
      page,
      baseURL,
      resultId,
    }) => {
      await page.setViewportSize({ width: w, height: h });
      await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
      const overflowed = await page.evaluate(
        () =>
          document.documentElement.scrollWidth >
          document.documentElement.clientWidth,
      );
      expect(overflowed).toBe(false);
    });

    test(`row 2: .main max-width <= 1600px at ${w}x${h}`, async ({
      page,
      baseURL,
      resultId,
    }) => {
      await page.setViewportSize({ width: w, height: h });
      await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
      const main = page.locator('.main').first();
      await expect(main).toBeVisible();
      const maxWidthStr = await main.evaluate(
        (el) => getComputedStyle(el).maxWidth,
      );
      const maxWidthPx = parsePx(maxWidthStr);
      expect(Number.isFinite(maxWidthPx)).toBe(true);
      expect(maxWidthPx).toBeLessThanOrEqual(1600);
    });

    test(`row 3: prose markdown-body max-width < 900px at ${w}x${h}`, async ({
      page,
      baseURL,
      resultId,
    }) => {
      await page.setViewportSize({ width: w, height: h });
      await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
      const prose = page
        .locator('.section-card--prose .markdown-body')
        .first();
      await expect(prose).toBeVisible({ timeout: 10_000 });
      const maxWidthStr = await prose.evaluate(
        (el) => getComputedStyle(el).maxWidth,
      );
      const maxWidthPx = parsePx(maxWidthStr);
      expect(Number.isFinite(maxWidthPx)).toBe(true);
      expect(maxWidthPx).toBeLessThan(900);
    });

    const snapTitle = `row 4: visual snapshot at ${w}x${h}`;
    if (process.env.UPDATE_SNAPSHOTS === '1') {
      test(snapTitle, async ({ page, baseURL, resultId }) => {
        await page.setViewportSize({ width: w, height: h });
        await page.goto(`${baseURL}/r/${resultId}`, {
          waitUntil: 'networkidle',
        });
        await expect(page).toHaveScreenshot(`main-${w}.png`);
      });
    } else {
      // eslint-disable-next-line playwright/no-skipped-test
      test.skip(`${snapTitle} (set UPDATE_SNAPSHOTS=1)`, () => {});
    }
  }
});
