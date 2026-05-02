import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import RerunButton from './RerunButton.svelte';
import { __resetCsrfToken } from './NewReviewDialog.svelte';

interface FetchCall {
  url: string;
  init?: RequestInit;
}

function makeFetch() {
  const calls: FetchCall[] = [];
  const fn = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input.toString();
    calls.push({ url, init });
    if (url.includes('/api/csrf-token')) {
      return new Response(JSON.stringify({ token: 'test-csrf' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }
    if (url.includes('/rerun')) {
      return new Response('{}', { status: 202, headers: { 'content-type': 'application/json' } });
    }
    return new Response('not found', { status: 404 });
  });
  return { fn, calls };
}

beforeEach(() => {
  __resetCsrfToken();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('RerunButton', () => {
  it('POSTs to per-section rerun URL with CSRF header', async () => {
    const { fn, calls } = makeFetch();
    vi.stubGlobal('fetch', fn);
    const { getByRole } = render(RerunButton, {
      props: { resultId: 'r123', groupId: 'g0', section: 'why' },
    });
    const btn = getByRole('button');
    await fireEvent.click(btn);
    // Allow promises to settle.
    await vi.advanceTimersByTimeAsync(0);

    const rerunCall = calls.find((c) => c.url.includes('/rerun'));
    expect(rerunCall).toBeDefined();
    expect(rerunCall!.url).toBe('/api/runs/r123/sections/g0/why/rerun');
    expect((rerunCall!.init?.method ?? '').toUpperCase()).toBe('POST');
    const headers = (rerunCall!.init?.headers ?? {}) as Record<string, string>;
    expect(headers['X-CSRF-Token']).toBe('test-csrf');
  });

  it('disabled briefly after success then re-enabled', async () => {
    const { fn } = makeFetch();
    vi.stubGlobal('fetch', fn);
    const { getByRole } = render(RerunButton, {
      props: { resultId: 'r123', groupId: 'g0', section: 'verdict' },
    });
    const btn = getByRole('button') as HTMLButtonElement;
    await fireEvent.click(btn);
    await vi.advanceTimersByTimeAsync(0);
    // After successful POST, cooldown is active.
    await waitFor(() => expect(btn.disabled).toBe(true));
    // Advance past 3s cooldown.
    await vi.advanceTimersByTimeAsync(3100);
    expect(btn.disabled).toBe(false);
  });

  it('encodes URL components', async () => {
    const { fn, calls } = makeFetch();
    vi.stubGlobal('fetch', fn);
    const { getByRole } = render(RerunButton, {
      props: { resultId: 'a/b', groupId: 'g 0', section: 'how' },
    });
    await fireEvent.click(getByRole('button'));
    await vi.advanceTimersByTimeAsync(0);
    const rerunCall = calls.find((c) => c.url.includes('/rerun'));
    expect(rerunCall!.url).toBe('/api/runs/a%2Fb/sections/g%200/how/rerun');
  });
});
