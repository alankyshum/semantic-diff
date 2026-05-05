/**
 * F13 — Structured VERDICT issues + Issues page (§3.8 of v2-browser-test-plan.md).
 *
 * Replay mode: sample-with-issues.json has 5 issues across 2 groups / 2 files.
 * The `--no-llm` live run produces zero verdict_issues, so this entire suite
 * runs against an isolated replay server spawned per-suite.
 *
 * Notes:
 * - Row 5 (group-filter) now exercises the #group-filter select against g0/g1.
 * - Row 7 (raw-verdict): plan originally said /issues but the codebase
 *   renders <details.raw-verdict> on /r/:id (review page). Test navigates
 *   to /r/:id to match the actual implementation.
 */
import { test, expect } from '../fixtures';
import { replayServer, type ReplayServer } from '../fixtures';

let replay: ReplayServer;

test.beforeAll(async () => {
  replay = await replayServer('tests/fixtures/results/sample-with-issues.json');
});

test.afterAll(async () => {
  await replay?.kill();
});

test.describe('F13 — issues page', () => {
  test('row 1: /r/:id tab strip exposes Issues (N) link with correct count', async ({ page }) => {
    await page.goto(`${replay.baseURL}/r/${replay.resultId}`);
    const tabStrip = page.locator('.tab-strip');
    await expect(tabStrip).toBeVisible();
    const issuesLink = tabStrip.locator(`a[href="/r/${replay.resultId}/issues"]`);
    await expect(issuesLink).toBeVisible();
    // Fixture has 5 issues total across both reviews (g0=3, g1=2).
    await expect(tabStrip.locator('.tab-count')).toHaveText('(5)');
  });

  test('row 2: /issues renders six severity checkboxes, all initially checked', async ({
    page,
  }) => {
    await page.goto(`${replay.baseURL}/r/${replay.resultId}/issues`);
    const sevChecks = page.locator('.filters .sev-check input[type="checkbox"]');
    await expect(sevChecks).toHaveCount(6);
    const checkedCount = await sevChecks.evaluateAll((nodes) =>
      nodes.filter((n) => (n as HTMLInputElement).checked).length,
    );
    expect(checkedCount).toBe(6);
  });

  test('row 3: unchecking all but critical filters URL + visible rows', async ({ page }) => {
    await page.goto(`${replay.baseURL}/r/${replay.resultId}/issues`);
    const labels = page.locator('.filters .sev-check');
    const count = await labels.count();
    // Uncheck every severity whose badge text is not "critical".
    for (let i = 0; i < count; i++) {
      const label = labels.nth(i);
      const text = (await label.innerText()).trim().toLowerCase();
      if (!text.includes('critical')) {
        await label.locator('input[type="checkbox"]').uncheck();
      }
    }
    // history.replaceState doesn't always fire frame URL-change events in
    // headless Chromium, so poll location.href directly.
    await expect.poll(
      () => page.evaluate(() => location.href),
      { timeout: 5_000 },
    ).toMatch(/severity=critical/);
    const visible = page.locator('article.issue');
    const visibleCount = await visible.count();
    expect(visibleCount).toBeGreaterThan(0);
    for (let i = 0; i < visibleCount; i++) {
      await expect(visible.nth(i).locator('.badge')).toHaveClass(/critical/);
    }
  });

  test('row 4: typing in #file-filter writes &file= and narrows rows', async ({ page }) => {
    await page.goto(`${replay.baseURL}/r/${replay.resultId}/issues`);
    await page.locator('#file-filter').fill('src/grouper/llm');
    // history.replaceState doesn't fire frame URL-change in headless Chromium.
    await expect.poll(
      () => page.evaluate(() => location.href),
      { timeout: 5_000 },
    ).toMatch(/file=src%2Fgrouper%2Fllm/);
    const articles = page.locator('article.issue');
    const n = await articles.count();
    expect(n).toBeGreaterThan(0);
    for (let i = 0; i < n; i++) {
      const filesText = await articles.nth(i).locator('.issue-files').innerText();
      expect(filesText).toContain('src/grouper/llm');
    }
  });

  test('row 5: #group-filter narrows by group', async ({ page }) => {
    await page.goto(`${replay.baseURL}/r/${replay.resultId}/issues`);
    const select = page.locator('#group-filter');
    await expect(select).toBeVisible();
    // Fixture has two groups: g0 ("LLM backend plumbing") and g1 ("Semantic grouper hardening").
    const options = select.locator('option');
    // "All groups" + g0 + g1 = 3 options
    await expect(options).toHaveCount(3);

    // Select g1 — should filter to only g1 issues (RV-2, RV-4).
    await select.selectOption('g1');
    await expect.poll(
      () => page.evaluate(() => location.href),
      { timeout: 5_000 },
    ).toMatch(/group=g1/);
    const articles = page.locator('article.issue');
    const n = await articles.count();
    expect(n).toBe(2);
    // Each visible issue should link back to g1's group.
    for (let i = 0; i < n; i++) {
      const groupLink = articles.nth(i).locator('a.issue-group');
      const href = await groupLink.getAttribute('href');
      expect(href).toContain(`/r/${replay.resultId}#issue-`);
    }

    // Switch back to "All groups" — all 5 issues should reappear.
    await select.selectOption('');
    const allCount = await articles.count();
    expect(allCount).toBe(5);
  });

  test('row 6: clicking .issue-group navigates to /r/:id#issue-{id}', async ({ page }) => {
    await page.goto(`${replay.baseURL}/r/${replay.resultId}/issues`);
    const firstIssue = page.locator('article.issue').first();
    const link = firstIssue.locator('a.issue-group');
    const href = await link.getAttribute('href');
    expect(href).toMatch(new RegExp(`^/r/${replay.resultId}#issue-`));
    const issueIdMatch = href!.match(/#issue-(.+)$/);
    expect(issueIdMatch).not.toBeNull();
    const issueId = issueIdMatch![1];

    await link.click();
    await expect(page).toHaveURL(new RegExp(`/r/${replay.resultId}#issue-${issueId}$`));
    const target = page.locator(`article.issue#issue-${issueId}`);
    await expect(target).toBeVisible();
  });

  test('row 7: details.raw-verdict on /r/:id is initially collapsed', async ({
    page,
  }) => {
    // raw-verdict is rendered on the review page (/r/:id) inside the VERDICT
    // section, not on /r/:id/issues. Navigating to the review page instead.
    await page.goto(`${replay.baseURL}/r/${replay.resultId}`);
    const details = page.locator('details.raw-verdict');
    await expect(details).toBeVisible();
    expect(await details.evaluate((el) => (el as HTMLDetailsElement).open)).toBe(false);
    await details.locator('summary').click();
    expect(await details.evaluate((el) => (el as HTMLDetailsElement).open)).toBe(true);
  });
});
