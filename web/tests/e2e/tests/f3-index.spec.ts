import { test, expect } from '../fixtures';

// Plan §3.5 — F3 Index repo grouping
//
// All rows run against the live `--no-llm` server. Row 4 is a negative test
// confirming `?repo=<name>` does NOT filter the index (current behavior; see
// roadmap deviation §1: F3 chip links to /?repo=… but index does not filter).

interface ResultSummary {
  id: string;
  repo_name?: string | null;
  title?: string;
  created_at: string;
  status: string;
}

test.describe('F3 — index repo grouping', () => {
  test('row 1: number of .repo-card matches distinct repo_name count', async ({
    page,
    baseURL,
    request,
  }) => {
    const res = await request.get(`${baseURL}/api/results`);
    expect(res.ok()).toBeTruthy();
    const results = (await res.json()) as ResultSummary[];
    const distinctRepos = new Set(
      results.map((r) => r.repo_name ?? 'Unknown repo'),
    );

    await page.goto(`${baseURL}/`, { waitUntil: 'domcontentloaded' });
    const cardCount = await page.locator('section.repo-card[aria-label]').count();
    expect(cardCount).toBe(distinctRepos.size);
  });

  test('row 2: each .repo-card .repo-name + .repo-count matches its group', async ({
    page,
    baseURL,
    request,
  }) => {
    const res = await request.get(`${baseURL}/api/results`);
    const results = (await res.json()) as ResultSummary[];
    const groupSizes = new Map<string, number>();
    for (const r of results) {
      const key = r.repo_name ?? 'Unknown repo';
      groupSizes.set(key, (groupSizes.get(key) ?? 0) + 1);
    }

    await page.goto(`${baseURL}/`, { waitUntil: 'domcontentloaded' });
    const cards = page.locator('section.repo-card[aria-label]');
    const count = await cards.count();
    expect(count).toBeGreaterThan(0);

    for (let i = 0; i < count; i++) {
      const card = cards.nth(i);
      const ariaLabel = await card.getAttribute('aria-label');
      expect(ariaLabel).not.toBeNull();
      const nameText = (await card.locator('.repo-name').textContent())?.trim() ?? '';
      expect(nameText).toBe(ariaLabel);

      const countText = (await card.locator('.repo-count').textContent())?.trim() ?? '';
      const m = countText.match(/(\d+)/);
      expect(m, `repo-count text "${countText}" should contain a number`).not.toBeNull();
      const renderedCount = Number(m![1]);
      expect(renderedCount).toBe(groupSizes.get(ariaLabel!));
    }
  });

  test('row 3: clicking first mini-card navigates to /r/<id>', async ({
    page,
    baseURL,
  }) => {
    await page.goto(`${baseURL}/`, { waitUntil: 'domcontentloaded' });
    const firstMini = page
      .locator('section.repo-card .repo-grid > a.mini-card')
      .first();
    await expect(firstMini).toBeVisible();
    const href = await firstMini.getAttribute('href');
    expect(href).toMatch(/^\/r\/[0-9a-f]{8}$/);

    await firstMini.click();
    await page.waitForURL(/\/r\/[0-9a-f]{8}/, { timeout: 10_000 });
    expect(page.url()).toMatch(new RegExp(`${href}$`));

    // Detail page <h1> exists (reflects the result title).
    const h1 = page.locator('h1').first();
    await expect(h1).toBeVisible({ timeout: 10_000 });
    const title = (await h1.textContent())?.trim() ?? '';
    expect(title.length).toBeGreaterThan(0);
  });

  test('row 4 (negative): /?repo=<name> does NOT filter the index', async ({
    page,
    baseURL,
  }) => {
    // First gather baseline card count.
    await page.goto(`${baseURL}/`, { waitUntil: 'domcontentloaded' });
    const baselineCount = await page
      .locator('section.repo-card[aria-label]')
      .count();
    expect(baselineCount).toBeGreaterThan(0);

    // Navigate to a result detail page to access the .repo-chip.
    const firstMini = page
      .locator('section.repo-card .repo-grid > a.mini-card')
      .first();
    const detailHref = await firstMini.getAttribute('href');
    await page.goto(`${baseURL}${detailHref}`, { waitUntil: 'domcontentloaded' });

    const chip = page.locator('a.repo-chip[href^="/?repo="]').first();
    await expect(chip).toBeVisible();
    const chipHref = await chip.getAttribute('href');
    expect(chipHref).toMatch(/^\/\?repo=/);

    await chip.click();
    await page.waitForURL(/\/\?repo=/, { timeout: 10_000 });
    expect(page.url()).toContain('?repo=');

    // Negative assertion: full set of repo cards still rendered (no filter).
    await expect(
      page.locator('section.repo-card[aria-label]'),
    ).toHaveCount(baselineCount);
  });
});
