import { test, expect } from '../fixtures';
import { replayServer, type ReplayServer } from '../fixtures';

// Plan §3.6 — F6 Run metadata + provenance.
//
// Rows 1–4 + 5 run against the live `--no-llm` server (BASE_URL/RESULT_ID).
// Row 6 spawns an isolated replay server fed by sample-with-issues.json which
// has `metadata.tokens` populated. Note: actual TokenUsage schema is
// { input_tokens, output_tokens, cost_usd } — RunMetadataPanel.svelte renders
// rows labelled "Input", "Output", "Cost", NOT prompt/completion/total.

test.describe('F6 — run metadata (live --no-llm)', () => {
  test('row 1: clicking "Run details" summary opens the panel', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const details = page.locator('details.run-details').first();
    await expect(details).toBeVisible();

    const summary = details.locator('> summary', { hasText: 'Run details' });
    await expect(summary).toBeVisible();
    await summary.click();

    await expect(details).toHaveAttribute('open', '');
    await expect(details.locator('.meta-panel')).toBeVisible();
  });

  test('row 2: dl.grid rows include tool_version, started_at, working_dir, cli_argv', async ({
    page,
    baseURL,
    resultId,
  }) => {
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const details = page.locator('details.run-details').first();
    await details.locator('> summary', { hasText: 'Run details' }).click();

    const meta = page.locator('.meta-panel');
    await expect(meta).toBeVisible();

    // Plan refers to JSON keys; UI labels in RunMetadataPanel.svelte are:
    // "Tool version", "Started", "Working dir", "CLI argv".
    const dtTexts = await meta.locator('dl.grid dt').allTextContents();
    const trimmed = dtTexts.map((s) => s.trim());
    expect(trimmed).toContain('Tool version');
    expect(trimmed).toContain('Started');
    expect(trimmed).toContain('Working dir');
    expect(trimmed).toContain('CLI argv');
  });

  test('row 3: clicking copy-btn calls navigator.clipboard.writeText with cli_argv', async ({
    page,
    baseURL,
    resultId,
  }) => {
    // Stub clipboard before page scripts execute.
    await page.addInitScript(() => {
      const calls: string[] = [];
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__clipboardCalls = calls;
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          writeText: (text: string) => {
            calls.push(text);
            return Promise.resolve();
          },
        },
      });
    });

    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const details = page.locator('details.run-details').first();
    await details.locator('> summary', { hasText: 'Run details' }).click();

    const argvCode = page.locator('.meta-panel .argv-row code.argv').first();
    await expect(argvCode).toBeVisible();
    const shown = (await argvCode.textContent())?.trim() ?? '';
    expect(shown.length).toBeGreaterThan(0);

    const copyBtn = page.locator(
      '.meta-panel .argv-row button.copy-btn[aria-label="Copy CLI argv"]',
    );
    await expect(copyBtn).toBeVisible();
    await copyBtn.click();

    // Allow promise microtask to drain.
    await page.waitForFunction(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      () => Array.isArray((window as any).__clipboardCalls) && (window as any).__clipboardCalls.length > 0,
      undefined,
      { timeout: 5_000 },
    );
    const calls: string[] = await page.evaluate(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      () => (window as any).__clipboardCalls,
    );
    expect(calls.length).toBe(1);
    expect(calls[0]).toBe(shown);
  });

  test('row 4: when skill_files non-empty, each row shows name + truncated hash', async ({
    page,
    baseURL,
    resultId,
    request,
  }) => {
    const res = await request.get(`${baseURL}/api/result/${resultId}`);
    expect(res.ok()).toBeTruthy();
    const doc = (await res.json()) as {
      metadata?: { skill_files?: Array<{ name: string; hash_blake3: string }> };
    };
    const skills = doc.metadata?.skill_files ?? [];
    test.skip(skills.length === 0, 'No skill_files on live --no-llm result; row 4 N/A');

    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const details = page.locator('details.run-details').first();
    await details.locator('> summary', { hasText: 'Run details' }).click();

    const items = page.locator('.meta-panel ul.skills > li');
    await expect(items).toHaveCount(skills.length);

    for (let i = 0; i < skills.length; i++) {
      const li = items.nth(i);
      const name = (await li.locator('.skill-name').textContent())?.trim() ?? '';
      expect(name).toBe(skills[i].name);

      const hashText = (await li.locator('.skill-hash').textContent())?.trim() ?? '';
      // ≤ 12 hex chars per plan
      expect(hashText.length).toBeGreaterThan(0);
      expect(hashText.length).toBeLessThanOrEqual(12);
      expect(hashText).toMatch(/^[0-9a-f]+$/);
      expect(skills[i].hash_blake3.startsWith(hashText)).toBe(true);
    }
  });

  test('row 5 (live --no-llm): metadata.tokens absent in API and tokens-block not rendered', async ({
    page,
    baseURL,
    resultId,
    request,
  }) => {
    const res = await request.get(`${baseURL}/api/result/${resultId}`);
    expect(res.ok()).toBeTruthy();
    const doc = (await res.json()) as { metadata?: { tokens?: unknown } };
    expect(doc.metadata?.tokens ?? null).toBeNull();

    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const details = page.locator('details.run-details').first();
    await details.locator('> summary', { hasText: 'Run details' }).click();

    await expect(page.locator('[data-testid="tokens-block"]')).toHaveCount(0);
  });
});

test.describe('F6 — run metadata (replay mode)', () => {
  let replay: ReplayServer | null = null;

  test.afterAll(async () => {
    if (replay) await replay.kill();
  });

  test('row 6: replay fixture renders Tokens block with input/output/cost rows', async ({
    page,
  }) => {
    replay = await replayServer('tests/fixtures/results/sample-with-issues.json');
    await page.goto(`${replay.baseURL}/r/${replay.resultId}`, {
      waitUntil: 'domcontentloaded',
    });

    const details = page.locator('details.run-details').first();
    await expect(details).toBeVisible({ timeout: 10_000 });
    await details.locator('> summary', { hasText: 'Run details' }).click();

    const block = page.locator('[data-testid="tokens-block"]');
    await expect(block).toHaveCount(1);
    await expect(block).toBeVisible();

    // Actual schema is { input_tokens, output_tokens, cost_usd }.
    // RunMetadataPanel renders dt labels: "Input", "Output", "Cost".
    const dtTexts = (await block.locator('dt').allTextContents()).map((s) =>
      s.trim(),
    );
    expect(dtTexts).toContain('Input');
    expect(dtTexts).toContain('Output');
    expect(dtTexts).toContain('Cost');

    // Fixture values: input_tokens=1234, output_tokens=567, cost_usd=0.0234 → "$0.0234".
    const ddTexts = (await block.locator('dd').allTextContents()).map((s) =>
      s.trim(),
    );
    expect(ddTexts).toContain('1234');
    expect(ddTexts).toContain('567');
    expect(ddTexts.some((t) => t.startsWith('$'))).toBe(true);
  });
});
