import { test, expect } from '../fixtures';

// Plan §3.7 — F9 Review/Diff/Split view toggle.
//
// All rows run against the live `--no-llm` server (BASE_URL/RESULT_ID) except
// row 8 which requires a group whose `unified_diff` is empty. The committed
// replay fixture (sample-with-issues.json) only has one group (g0) with a
// populated `unified_diff`, and the live --no-llm result also has populated
// diffs, so row 8 is gated as test.skip when no empty-diff group exists.

test.describe('F9 — view-mode toggle', () => {
  test('row 1: tablist contains 3 view-btn buttons; default = Review', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    const tablist = page.locator('[role="tablist"][aria-label="View mode"]');
    await expect(tablist).toBeVisible();

    const buttons = tablist.locator('button.view-btn');
    await expect(buttons).toHaveCount(3);

    const labels = (await buttons.allTextContents()).map((s) => s.trim());
    expect(labels).toEqual(['Review', 'Split', 'Diff']);

    // Default active = Review.
    const review = tablist.locator('button.view-btn', { hasText: 'Review' });
    await expect(review).toHaveClass(/\bactive\b/);
    await expect(review).toHaveAttribute('aria-selected', 'true');
  });

  test('row 2: at 1400x900, all three tabs enabled (no .disabled)', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    const buttons = page.locator(
      '[role="tablist"][aria-label="View mode"] button.view-btn',
    );
    const count = await buttons.count();
    expect(count).toBe(3);
    for (let i = 0; i < count; i++) {
      await expect(buttons.nth(i)).not.toHaveClass(/\bdisabled\b/);
    }
  });

  test('row 3: at 1024x768, Split has disabled class and clicking is no-op', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    const split = page.locator(
      '[role="tablist"][aria-label="View mode"] button.view-btn',
      { hasText: 'Split' },
    );
    await expect(split).toBeVisible();
    await expect(split).toHaveClass(/\bdisabled\b/);

    // Capture current view mode before click.
    const before = await page.evaluate(
      (k) => localStorage.getItem(k),
      `view-mode:${resultId}`,
    );
    await split.click({ force: true });
    // No-op: aria-selected on Split should remain false; localStorage unchanged.
    await expect(split).toHaveAttribute('aria-selected', 'false');
    const after = await page.evaluate(
      (k) => localStorage.getItem(k),
      `view-mode:${resultId}`,
    );
    expect(after).toBe(before);
  });

  test('row 4: clicking Diff renders DiffViewer; section-card hidden; localStorage = diff', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    const diffBtn = page.locator(
      '[role="tablist"][aria-label="View mode"] button.view-btn',
      { hasText: 'Diff' },
    );
    await diffBtn.click();
    await expect(diffBtn).toHaveClass(/\bactive\b/);

    // DiffViewer rendered — at least one hunk row visible (heuristic: any
    // line within the .section-card's diff content). Use generic selector
    // since DiffViewer markup varies.
    const sectionCard = page.locator('main.main .section-card').first();
    await expect(sectionCard).toBeVisible();

    // Review-mode multi-card layout (multiple .section-card--prose) should be gone.
    await expect(page.locator('main.main .section-card--prose')).toHaveCount(0);

    const stored = await page.evaluate(
      (k) => localStorage.getItem(k),
      `view-mode:${resultId}`,
    );
    expect(stored).toBe('diff');
  });

  test('row 5: clicking Review renders section-card; localStorage = review', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    // Switch first to diff, then back to review, to assert the toggle.
    await page
      .locator('[role="tablist"][aria-label="View mode"] button.view-btn', {
        hasText: 'Diff',
      })
      .click();
    await page
      .locator('[role="tablist"][aria-label="View mode"] button.view-btn', {
        hasText: 'Review',
      })
      .click();

    await expect(page.locator('main.main .section-card--prose').first()).toBeVisible();

    const stored = await page.evaluate(
      (k) => localStorage.getItem(k),
      `view-mode:${resultId}`,
    );
    expect(stored).toBe('review');
  });

  test('row 6: at 1400, pressing v cycles review → split → diff → review', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    // Force initial state to review.
    await page.evaluate((k) => localStorage.setItem(k, 'review'), `view-mode:${resultId}`);
    await page.reload({ waitUntil: 'domcontentloaded' });

    // Ensure body has focus (no editable element).
    await page.locator('body').click({ position: { x: 1, y: 1 } });

    const readMode = async () =>
      page.evaluate((k) => localStorage.getItem(k), `view-mode:${resultId}`);

    await page.keyboard.press('v');
    expect(await readMode()).toBe('split');
    await page.keyboard.press('v');
    expect(await readMode()).toBe('diff');
    await page.keyboard.press('v');
    expect(await readMode()).toBe('review');
    await page.keyboard.press('v');
    expect(await readMode()).toBe('split');
  });

  test('row 7: at 1024, pressing v cycles review → diff → review (Split skipped)', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    await page.evaluate((k) => localStorage.setItem(k, 'review'), `view-mode:${resultId}`);
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.locator('body').click({ position: { x: 1, y: 1 } });

    const readMode = async () =>
      page.evaluate((k) => localStorage.getItem(k), `view-mode:${resultId}`);

    await page.keyboard.press('v');
    expect(await readMode()).toBe('diff');
    await page.keyboard.press('v');
    expect(await readMode()).toBe('review');
    await page.keyboard.press('v');
    expect(await readMode()).toBe('diff');
  });

  test('row 8: empty unified_diff group shows .diff-fallback', async ({
    page,
    baseURL,
    resultId,
    request,
  }) => {
    const res = await request.get(`${baseURL}/api/result/${resultId}`);
    const doc = (await res.json()) as {
      groups: Array<{ id: string; unified_diff?: string }>;
    };
    const emptyGroup = doc.groups.find(
      (g) => !g.unified_diff || g.unified_diff.trim() === '',
    );
    test.skip(
      !emptyGroup,
      'No group with empty unified_diff in live result; replay fixture sample-with-issues.json also has populated diff. Documented gap.',
    );

    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    // Select the empty-diff group via sidebar (if multiple groups) — fall back
    // to localStorage-driven selection is not supported; click the sidebar row.
    const sbRow = page.locator(`#sb-group-${emptyGroup!.id}`);
    if ((await sbRow.count()) > 0) {
      await sbRow.click();
    }

    await page
      .locator('[role="tablist"][aria-label="View mode"] button.view-btn', {
        hasText: 'Diff',
      })
      .click();

    await expect(page.locator('p.diff-fallback')).toBeVisible();
  });

  test('row 9: input-focus guard — pressing v while input is focused does not change view', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });

    // Ensure baseline is review.
    await page.evaluate((k) => localStorage.setItem(k, 'review'), `view-mode:${resultId}`);
    await page.reload({ waitUntil: 'domcontentloaded' });

    const before = await page.evaluate(
      (k) => localStorage.getItem(k),
      `view-mode:${resultId}`,
    );

    // Inject a temp <input> and focus it (plan §3.7 row 9 fallback path).
    await page.evaluate(() => {
      const inp = document.createElement('input');
      inp.type = 'text';
      inp.id = '__e2e_temp_input';
      inp.style.position = 'fixed';
      inp.style.top = '0';
      inp.style.left = '0';
      inp.style.zIndex = '9999';
      document.body.appendChild(inp);
      inp.focus();
    });

    // Confirm focus landed on the input.
    const tag = await page.evaluate(
      () => (document.activeElement as HTMLElement | null)?.tagName ?? null,
    );
    expect(tag).toBe('INPUT');

    await page.keyboard.press('v');

    const after = await page.evaluate(
      (k) => localStorage.getItem(k),
      `view-mode:${resultId}`,
    );
    expect(after).toBe(before);

    // Active button should still be Review.
    await expect(
      page.locator('[role="tablist"][aria-label="View mode"] button.view-btn.active'),
    ).toHaveText('Review');
  });
});
