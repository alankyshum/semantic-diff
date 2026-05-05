/**
 * F5 — Settings UI (§3.10 of v2-browser-test-plan.md).
 *
 * Runs against the live `--no-llm` baseURL fixture; /settings is independent
 * of any specific result. The save round-trip restores the baseline at suite
 * end so other suites are not polluted.
 *
 * Notes:
 * - Row 7 (chip reorder) attempts a "Move <name> up" click only if a chip
 *   with such a control is enabled & visible; otherwise skips.
 * - Row 11 is explicitly deferred per the plan.
 */
import { test, expect } from '../fixtures';

let baseline: { raw?: unknown } | null = null;
let csrf = '';

test.describe('F5 — settings UI', () => {
  test.beforeAll(async ({ playwright, baseURL }) => {
    const ctx = await playwright.request.newContext();
    const tokenRes = await ctx.get(`${baseURL}/api/csrf-token`);
    csrf = ((await tokenRes.json()) as { token: string }).token;
    const cfgRes = await ctx.get(`${baseURL}/api/config`);
    baseline = (await cfgRes.json()) as { raw?: unknown };
    await ctx.dispose();
  });

  test.afterAll(async ({ playwright, baseURL }) => {
    if (!baseline) return;
    const ctx = await playwright.request.newContext();
    try {
      const payload = baseline.raw ?? baseline;
      await ctx.put(`${baseURL}/api/config`, {
        headers: { 'X-CSRF-Token': csrf, 'content-type': 'application/json' },
        data: payload as Record<string, unknown>,
      });
    } finally {
      await ctx.dispose();
    }
  });

  test('row 1: clicking gear icon on / navigates to /settings', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}/`);
    const gear = page.locator('a.icon-link[href="/settings"]');
    await expect(gear).toBeVisible();
    await gear.click();
    await expect(page).toHaveURL(/\/settings$/);
  });

  test('row 2: form.settings-form rendered and #sf-preferred matches /api/config', async ({
    page,
    request,
    baseURL,
  }) => {
    await page.goto(`${baseURL}/settings`);
    await expect(page.locator('form.settings-form')).toBeVisible();
    const apiRes = await request.get(`${baseURL}/api/config`);
    const apiBody = await apiRes.json();
    const expected = (apiBody?.raw?.['preferred-ai-cli'] ?? '') as string;
    const select = page.locator('#sf-preferred');
    await expect(select).toHaveValue(expected);
  });

  test('row 3: model fields populated from API', async ({ page, request, baseURL }) => {
    await page.goto(`${baseURL}/settings`);
    const apiBody = await (await request.get(`${baseURL}/api/config`)).json();
    const raw = apiBody?.raw ?? {};
    await expect(page.locator('#sf-claude-model')).toHaveValue(raw?.claude?.model ?? '');
    await expect(page.locator('#sf-copilot-model')).toHaveValue(raw?.copilot?.model ?? '');
    await expect(page.locator('#sf-cursor-model')).toHaveValue(raw?.cursor?.model ?? '');
  });

  test('row 4: editing #sf-claude-model enables the Save button', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}/settings`);
    const save = page.locator('form.settings-form .btn-primary[type="submit"]');
    await expect(save).toBeDisabled();
    await page.locator('#sf-claude-model').fill('e2e-row4-marker');
    await expect(save).toBeEnabled();
  });

  test('row 5+6: clicking Save fires exactly one PUT /api/config and shows banner', async ({
    page,
    baseURL,
  }) => {
    await page.goto(`${baseURL}/settings`);
    const putRequests: string[] = [];
    // Forward through; do not block.
    await page.route('**/api/config', async (route) => {
      const req = route.request();
      if (req.method() === 'PUT') {
        const csrfHdr = req.headers()['x-csrf-token'] ?? '';
        putRequests.push(csrfHdr);
      }
      await route.continue();
    });

    await page.locator('#sf-claude-model').fill(`e2e-row5-${Date.now()}`);
    await page.locator('form.settings-form .btn-primary[type="submit"]').click();

    // Banner appears on 200.
    await expect(page.locator('.banner.banner-ok[role="status"]')).toBeVisible();
    expect(putRequests.length).toBe(1);
    expect(putRequests[0].length).toBeGreaterThan(0);
  });

  test('row 7: chip reorder via "Move <name> up" if such a control is enabled', async ({
    page,
    baseURL,
  }) => {
    await page.goto(`${baseURL}/settings`);
    // Find any enabled "Move <name> up" button (the first chip's up button is
    // always disabled, so we need an enabled one further down the list).
    const upButtons = page.locator('button[aria-label^="Move "][aria-label$=" up"]');
    const count = await upButtons.count();
    let firstEnabledIdx = -1;
    for (let i = 0; i < count; i++) {
      if (await upButtons.nth(i).isVisible() && (await upButtons.nth(i).isEnabled())) {
        firstEnabledIdx = i;
        break;
      }
    }
    if (firstEnabledIdx < 0) {
      test.skip(true, 'no enabled "Move <name> up" control rendered with current config');
      return;
    }
    const chipsBefore = await page
      .locator('.chip[data-provider]')
      .evaluateAll((nodes) => nodes.map((n) => n.getAttribute('data-provider')));
    await upButtons.nth(firstEnabledIdx).click();
    const chipsAfter = await page
      .locator('.chip[data-provider]')
      .evaluateAll((nodes) => nodes.map((n) => n.getAttribute('data-provider')));
    expect(chipsAfter).not.toEqual(chipsBefore);
    await expect(page.locator('form.settings-form .btn-primary[type="submit"]')).toBeEnabled();
  });

  test('row 8: dirtying then clicking Reset reverts and disables Save', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}/settings`);
    const save = page.locator('form.settings-form .btn-primary[type="submit"]');
    const reset = page.locator('form.settings-form button[type="button"]', { hasText: 'Reset' });
    const claudeInput = page.locator('#sf-claude-model');
    const original = (await claudeInput.inputValue()) ?? '';
    await claudeInput.fill('row8-dirty');
    await expect(save).toBeEnabled();
    await reset.click();
    await expect(claudeInput).toHaveValue(original);
    await expect(save).toBeDisabled();
  });

  test('row 9: provider probe section renders .bin-row entries with badges', async ({
    page,
    baseURL,
  }) => {
    await page.goto(`${baseURL}/settings`);
    // Wait for probe to finish (or fail).
    await page.waitForFunction(() => {
      return !document.body.innerText.includes('Probing providers');
    });
    const rows = page.locator('.bin-row');
    const n = await rows.count();
    expect(n).toBeGreaterThan(0);
    for (let i = 0; i < n; i++) {
      await expect(rows.nth(i).locator('.badge')).toHaveCount(1, { timeout: 0 }).catch(async () => {
        // Some rows have multiple badges (found + version-status). At least one is required.
        const badgeCount = await rows.nth(i).locator('.badge').count();
        expect(badgeCount).toBeGreaterThan(0);
      });
    }
  });

  test('row 10: clicking Re-detect fires one additional GET /api/config/probe', async ({
    page,
    baseURL,
  }) => {
    await page.goto(`${baseURL}/settings`);
    await page.waitForFunction(() => !document.body.innerText.includes('Probing providers'));
    let probeGets = 0;
    await page.route('**/api/config/probe', async (route) => {
      if (route.request().method() === 'GET') probeGets++;
      await route.continue();
    });
    await page.locator('button.btn.btn-small.refresh').click();
    await page.waitForFunction(() => !document.body.innerText.includes('Probing providers'));
    expect(probeGets).toBe(1);
  });

  test.skip('row 11: effective-config diff preview / external-edit toast (deferred — not implemented)', async () => {
    // Per §3.10 plan note + §1 deferred list: F5 effective-config diff preview
    // and external-edit detection are not shipped; deliberately not tested.
  });
});
