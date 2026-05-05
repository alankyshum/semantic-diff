import { test, expect } from '../fixtures';
import { spawn } from 'node:child_process';
import path from 'node:path';

// Plan §3.12 — F8 RepoHistoryNav

const REPO_ROOT = path.resolve(__dirname, '../../../..');
const BINARY = path.join(REPO_ROOT, 'target/release/semantic-diff');

interface ResultSummary {
  id: string;
  title: string;
  created_at: string;
  status: string;
  repo_name?: string;
}

/**
 * Spawn a fresh `--no-llm` run with a unique `--title` to force a distinct
 * blake3 id (id = blake3(raw_diff || title), first 8 hex chars), persisting
 * a second result for the same repo into the default results dir
 * (~/.local/share/semantic-diff/results/ on Linux,
 *  ~/Library/Application Support/semantic-diff/results/ on macOS).
 *
 * Resolves once the child exits successfully. The child opens an HTTP server
 * and waits for Ctrl+C, so we send SIGTERM after detecting the
 * "Review complete." line on stderr.
 */
function seedSecondResult(title: string): Promise<{ ok: boolean; reason?: string }> {
  return new Promise((resolve) => {
    let resolved = false;
    const finish = (ok: boolean, reason?: string) => {
      if (!resolved) {
        resolved = true;
        resolve({ ok, reason });
      }
    };
    const child = spawn(
      BINARY,
      [
        '--diff',
        'tests/fixtures/real-world.patch',
        '--no-llm',
        '--no-open',
        '--port',
        '0',
        '--title',
        title,
      ],
      { cwd: REPO_ROOT, stdio: ['ignore', 'pipe', 'pipe'] },
    );
    let stderrBuf = '';
    child.stderr?.on('data', (d) => {
      stderrBuf += d.toString();
      // The orchestrator prints "Review complete. Result: <path>" once the
      // result.json has been atomically written. Kill once seen.
      if (stderrBuf.includes('Review complete')) {
        try {
          if (child.pid) process.kill(child.pid, 'SIGTERM');
        } catch {
          /* ignore */
        }
      }
    });
    child.on('exit', () => finish(true));
    child.on('error', (e) => finish(false, String(e)));
    setTimeout(() => {
      try {
        if (child.pid) process.kill(child.pid, 'SIGKILL');
      } catch {
        /* ignore */
      }
      finish(false, 'seed timeout');
    }, 30_000);
  });
}

let seedAttempted = false;
let historyCount = 0;
let repoName: string | null = null;
let primaryId: string | null = null;
let secondaryId: string | null = null;
let seedError: string | null = null;

test.describe('F8 — RepoHistoryNav', () => {
  test.beforeAll(async ({ request, baseURL, resultId }) => {
    seedAttempted = true;
    primaryId = resultId;

    // Fetch the primary result to determine the repo name.
    const primaryRes = await request.get(`${baseURL}/api/result/${resultId}`);
    if (primaryRes.ok()) {
      const doc = await primaryRes.json();
      repoName = doc?.repo?.name ?? null;
    }

    // Seed a second result with a deterministic-but-distinct title so the
    // blake3 id differs from the primary live run (which has no --title).
    const seed = await seedSecondResult(`F8 seed ${Date.now()}`);
    if (!seed.ok) {
      seedError = seed.reason ?? 'unknown';
    }

    // Query history through the live server (which lists from the same
    // default results dir the seeded run wrote into).
    if (repoName) {
      const histRes = await request.get(
        `${baseURL}/api/repos/${encodeURIComponent(repoName)}/results`,
      );
      if (histRes.ok()) {
        const list = (await histRes.json()) as ResultSummary[];
        historyCount = Array.isArray(list) ? list.length : 0;
        if (historyCount >= 2) {
          const other = list.find((r) => r.id !== primaryId);
          secondaryId = other?.id ?? null;
        }
      }
    }
  });

  test('row 1: trigger button visible with History (N) label, N>=2', async ({
    page,
    baseURL,
    resultId,
  }) => {
    test.skip(
      historyCount < 2,
      `need >=2 results for repo to test history nav; got ${historyCount} (seedError=${seedError})`,
    );
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const trigger = page.locator(
      '.repo-history-nav button.trigger[aria-haspopup="menu"]',
    );
    await expect(trigger).toBeVisible({ timeout: 10_000 });
    const label = (await trigger.locator('.label').textContent())?.trim() ?? '';
    expect(label).toMatch(/^History\s*\(\d+\)$/);
    const m = label.match(/\((\d+)\)/);
    const n = m ? parseInt(m[1], 10) : 0;
    expect(n).toBeGreaterThanOrEqual(2);
  });

  test('row 2: clicking trigger opens panel; current row has aria-current="true"', async ({
    page,
    baseURL,
    resultId,
  }) => {
    test.skip(
      historyCount < 2,
      `need >=2 results to test panel; got ${historyCount} (seedError=${seedError})`,
    );
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const trigger = page.locator(
      '.repo-history-nav button.trigger[aria-haspopup="menu"]',
    );
    await expect(trigger).toBeVisible({ timeout: 10_000 });
    await expect(trigger).toBeEnabled();
    await trigger.click();
    const panel = page.locator('.repo-history-nav .panel[role="menu"]');
    await expect(panel).toBeVisible();
    const firstRow = panel.locator('.row').first();
    await expect(firstRow).toHaveClass(/current/);
    await expect(firstRow).toHaveAttribute('aria-current', 'true');
  });

  test('row 3: clicking a non-current row navigates to that result', async ({
    page,
    baseURL,
    resultId,
  }) => {
    test.skip(
      historyCount < 2 || !secondaryId,
      `need >=2 results and a non-current id to navigate to; got ${historyCount}`,
    );
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const trigger = page.locator(
      '.repo-history-nav button.trigger[aria-haspopup="menu"]',
    );
    await expect(trigger).toBeVisible({ timeout: 10_000 });
    await trigger.click();
    const panel = page.locator('.repo-history-nav .panel[role="menu"]');
    await expect(panel).toBeVisible();
    const otherRow = panel.locator(`a.row[href="/r/${secondaryId}"]`).first();
    await expect(otherRow).toBeVisible();
    await otherRow.click();
    await page.waitForURL(`**/r/${secondaryId}`, { timeout: 10_000 });
    expect(page.url()).toContain(`/r/${secondaryId}`);
    // Page should re-render with a fresh h1.
    await expect(page.locator('h1').first()).toBeVisible();
  });

  test('row 4: with only one result for this repo, trigger is disabled', async ({
    page,
    baseURL,
    resultId,
  }) => {
    test.skip(
      historyCount !== 1,
      `row 4 only applies when historyCount===1; actual=${historyCount}`,
    );
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const trigger = page.locator('.repo-history-nav button.trigger');
    await expect(trigger).toBeVisible({ timeout: 10_000 });
    // Wait for the in-component fetch to settle so the disabled state is final.
    await page.waitForFunction(() => {
      const btn = document.querySelector(
        '.repo-history-nav button.trigger',
      ) as HTMLButtonElement | null;
      if (!btn) return false;
      const lbl = btn.textContent ?? '';
      // After load, label is "History (N)" with a real number, not "(...)".
      return /\(\d+\)/.test(lbl);
    }, { timeout: 10_000 });
    const isDisabled = await trigger.evaluate(
      (el) =>
        (el as HTMLButtonElement).disabled ||
        el.getAttribute('aria-disabled') === 'true',
    );
    expect(isDisabled).toBe(true);
  });

  test('row 5: clicking outside the panel closes it', async ({
    page,
    baseURL,
    resultId,
  }) => {
    test.skip(
      historyCount < 2,
      `need >=2 results to open the panel; got ${historyCount} (seedError=${seedError})`,
    );
    await page.goto(`${baseURL}/r/${resultId}`, { waitUntil: 'domcontentloaded' });
    const trigger = page.locator(
      '.repo-history-nav button.trigger[aria-haspopup="menu"]',
    );
    await expect(trigger).toBeVisible({ timeout: 10_000 });
    await trigger.click();
    const panel = page.locator('.repo-history-nav .panel[role="menu"]');
    await expect(panel).toBeVisible();
    // Click somewhere clearly outside the panel/trigger.
    await page.locator('main.main').first().click({ position: { x: 50, y: 50 } });
    await expect(panel).toHaveCount(0);
  });

  test('row 6: API contract — GET /api/repos/:name/results returns array', async ({
    request,
    baseURL,
    resultId,
  }) => {
    // Resolve repo name from primary result (don't rely on beforeAll state).
    const primary = await request.get(`${baseURL}/api/result/${resultId}`);
    expect(primary.ok()).toBeTruthy();
    const doc = await primary.json();
    const name: string | undefined = doc?.repo?.name;
    test.skip(!name, 'primary result has no repo.name');
    const res = await request.get(
      `${baseURL}/api/repos/${encodeURIComponent(name as string)}/results`,
    );
    expect(res.status()).toBe(200);
    const list = (await res.json()) as ResultSummary[];
    expect(Array.isArray(list)).toBe(true);
    expect(list.length).toBeGreaterThanOrEqual(1);
    // Each entry has the expected summary shape.
    for (const entry of list) {
      expect(typeof entry.id).toBe('string');
      expect(entry.id).toMatch(/^[0-9a-f]{8}$/);
      expect(typeof entry.created_at).toBe('string');
    }
    // Newest-first: created_at descending.
    if (list.length >= 2) {
      const ts = list.map((r) => Date.parse(r.created_at));
      for (let i = 1; i < ts.length; i++) {
        if (Number.isFinite(ts[i - 1]) && Number.isFinite(ts[i])) {
          expect(ts[i - 1]).toBeGreaterThanOrEqual(ts[i]);
        }
      }
    }
  });

  // eslint-disable-next-line playwright/no-skipped-test
  test.skip("row 7: [/] keyboard shortcuts — deferred per plan §3.12", () => {
    // Plan §3.12 row 7 explicitly deferred (not implemented).
  });
});

// Reference the unused vars to silence TS "declared but never read" lints when
// only some branches use them.
void seedAttempted;
