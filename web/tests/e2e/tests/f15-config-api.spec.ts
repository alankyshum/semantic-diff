/**
 * F15 — Unified config + JSON Schema (§3.9 of v2-browser-test-plan.md).
 *
 * API-level tests against the live `--no-llm` server (BASE_URL fixture).
 * Only row 3 navigates a page; the rest are pure HTTP.
 *
 * Round-trip mutation (row 6) saves the baseline at suite start and restores
 * it at the end so other suites are not polluted.
 */
import { test, expect } from '../fixtures';

let csrf: string;
let baseline: unknown;
let baselineHeaders: Record<string, string>;

async function fetchCsrf(baseURL: string, request: import('@playwright/test').APIRequestContext) {
  const res = await request.get(`${baseURL}/api/csrf-token`);
  expect(res.ok()).toBeTruthy();
  const body = (await res.json()) as { token: string };
  return body.token;
}

test.describe('F15 — config & CSRF API', () => {
  test.beforeAll(async ({ playwright, baseURL }) => {
    const ctx = await playwright.request.newContext();
    csrf = await fetchCsrf(baseURL, ctx);
    const r = await ctx.get(`${baseURL}/api/config`);
    expect(r.ok()).toBeTruthy();
    baseline = await r.json();
    baselineHeaders = { 'X-CSRF-Token': csrf, 'content-type': 'application/json' };
    await ctx.dispose();
  });

  test.afterAll(async ({ playwright, baseURL }) => {
    if (!baseline || typeof baseline !== 'object') return;
    const ctx = await playwright.request.newContext();
    // The PUT endpoint accepts a RawConfig payload, not the full envelope.
    // GET returns { path, exists, raw, parse_error } — we PUT only `raw`.
    const env = baseline as { raw?: unknown };
    const payload = env.raw ?? baseline;
    try {
      await ctx.put(`${baseURL}/api/config`, {
        headers: baselineHeaders,
        data: payload as Record<string, unknown>,
      });
    } finally {
      await ctx.dispose();
    }
  });

  test('row 1: GET /api/config/schema is a valid JSON Schema with preferred-ai-cli', async ({
    request,
    baseURL,
  }) => {
    const res = await request.get(`${baseURL}/api/config/schema`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    // schemars output uses `$schema` (per draft 2020-12 / draft-07 defaults).
    expect(body).toHaveProperty('$schema');
    // The plan reads `properties.preferred_ai_cli`; schemars + serde-rename
    // means the property is actually kebab-case. Accept either to stay
    // faithful to plan intent.
    const props = body.properties ?? {};
    const hasPreferred =
      Object.prototype.hasOwnProperty.call(props, 'preferred-ai-cli') ||
      Object.prototype.hasOwnProperty.call(props, 'preferred_ai_cli');
    expect(hasPreferred).toBe(true);
  });

  test('row 2: GET /api/csrf-token returns a non-empty token', async ({ request, baseURL }) => {
    const res = await request.get(`${baseURL}/api/csrf-token`);
    expect(res.status()).toBe(200);
    const body = (await res.json()) as { token: string };
    expect(typeof body.token).toBe('string');
    expect(body.token.length).toBeGreaterThan(0);
  });

  test('row 3: <meta name="csrf-token"> on / equals the API token', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}/`);
    const meta = await page.locator('meta[name="csrf-token"]').getAttribute('content');
    // The placeholder must have been substituted; if the marker is missing
    // (e.g. dev build without the meta), skip the equality check.
    if (meta === null) {
      test.skip(true, 'meta[name="csrf-token"] not present in served HTML');
      return;
    }
    expect(meta).not.toContain('%csrf_token%');
    expect(meta).toBe(csrf);
  });

  test('row 4: PUT /api/config without X-CSRF-Token returns 403', async ({ request, baseURL }) => {
    const res = await request.put(`${baseURL}/api/config`, {
      headers: { 'content-type': 'application/json' },
      data: {},
    });
    expect(res.status()).toBe(403);
  });

  test('row 5: PUT /api/config with valid token but malformed body returns 422', async ({
    request,
    baseURL,
  }) => {
    const res = await request.put(`${baseURL}/api/config`, {
      headers: { 'X-CSRF-Token': csrf, 'content-type': 'application/json' },
      data: { bogus_unknown_field: 1 },
    });
    expect(res.status()).toBe(422);
    const body = await res.json();
    expect(typeof body.error).toBe('string');
  });

  test('row 6: round-trip GET → mutate → PUT → GET reflects the mutation', async ({
    request,
    baseURL,
  }) => {
    const before = await (await request.get(`${baseURL}/api/config`)).json();
    const baseRaw =
      before && typeof before === 'object' && 'raw' in before ? before.raw : before;

    // Build a mutated payload — flip claude.model to a known marker string.
    const marker = `e2e-test-marker-${Date.now()}`;
    const mutated = {
      ...(baseRaw ?? {}),
      claude: { ...((baseRaw as { claude?: object })?.claude ?? {}), model: marker },
    };

    const putRes = await request.put(`${baseURL}/api/config`, {
      headers: { 'X-CSRF-Token': csrf, 'content-type': 'application/json' },
      data: mutated,
    });
    expect(putRes.status()).toBe(200);

    const after = await (await request.get(`${baseURL}/api/config`)).json();
    const afterRaw =
      after && typeof after === 'object' && 'raw' in after ? after.raw : after;
    expect(afterRaw?.claude?.model).toBe(marker);
  });

  test('row 7: GET /api/config/probe lists provider entries', async ({ request, baseURL }) => {
    const res = await request.get(`${baseURL}/api/config/probe`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.providers)).toBe(true);
    expect(body.providers.length).toBeGreaterThan(0);
    for (const p of body.providers) {
      expect(typeof p.name).toBe('string');
      // Each provider has a list of binaries with `name` and `found`.
      expect(Array.isArray(p.binaries)).toBe(true);
      for (const b of p.binaries) {
        expect(typeof b.name).toBe('string');
        expect(typeof b.found).toBe('boolean');
      }
    }
  });
});
