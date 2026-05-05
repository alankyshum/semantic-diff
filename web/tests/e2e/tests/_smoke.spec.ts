import { test, expect } from '../fixtures';

test('harness boots: BASE_URL and RESULT_ID injected', async ({ baseURL, resultId, request }) => {
  expect(baseURL).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(resultId).toMatch(/^[0-9a-f]{8}$/);
  const res = await request.get(`${baseURL}/api/results`);
  expect(res.ok()).toBeTruthy();
  const body = await res.json();
  expect(Array.isArray(body)).toBe(true);
  expect(body.length).toBeGreaterThan(0);
});
