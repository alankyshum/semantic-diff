import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import NewReviewDialog, { __resetCsrfToken, splitGitArgs } from './NewReviewDialog.svelte';
import type { PreviewResponse, RunResponse } from '$lib/types';

vi.mock('$app/navigation', () => ({
  goto: vi.fn(async () => {}),
}));

// jsdom doesn't implement HTMLDialogElement.showModal/close.
beforeEach(() => {
  __resetCsrfToken();
  if (!HTMLDialogElement.prototype.showModal) {
    HTMLDialogElement.prototype.showModal = function () {
      this.setAttribute('open', '');
      (this as unknown as { open: boolean }).open = true;
    };
  }
  if (!HTMLDialogElement.prototype.close) {
    HTMLDialogElement.prototype.close = function () {
      this.removeAttribute('open');
      (this as unknown as { open: boolean }).open = false;
    };
  }
});

afterEach(() => {
  vi.unstubAllGlobals();
});

interface FetchCall {
  url: string;
  init?: RequestInit;
}

function makeFetch(handlers: Record<string, () => Response | Promise<Response>>) {
  const calls: FetchCall[] = [];
  const fn = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input.toString();
    calls.push({ url, init });
    for (const [pattern, handler] of Object.entries(handlers)) {
      if (url.includes(pattern)) return handler();
    }
    return new Response('not found', { status: 404 });
  });
  return { fn, calls };
}

const csrfHandler = () =>
  new Response(JSON.stringify({ token: 'test-csrf' }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

const previewBody: PreviewResponse = {
  groups: [
    {
      group_id: 'g0',
      title: 'Group 0',
      sections: {
        WHY: { input_tokens: 100, output_tokens_est: 50, cost_usd: 0.01 },
        WHAT: { input_tokens: 200, output_tokens_est: 80, cost_usd: 0.02 },
        HOW: { input_tokens: 150, output_tokens_est: 60, cost_usd: 0.015 },
        VERDICT: { input_tokens: 300, output_tokens_est: 120, cost_usd: 0.04 },
      },
    },
  ],
  total_input_tokens: 750,
  total_output_tokens_est: 310,
  total_cost_usd: 0.085,
};

describe('NewReviewDialog', () => {
  it('disables Run review when git refs empty', async () => {
    vi.stubGlobal('fetch', makeFetch({ '/api/csrf-token': csrfHandler }).fn);
    const { component, getByText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    const run = getByText('Run review') as HTMLButtonElement;
    expect(run.disabled).toBe(true);
  });

  it('enables Run review when git refs provided', async () => {
    vi.stubGlobal('fetch', makeFetch({ '/api/csrf-token': csrfHandler }).fn);
    const { component, getByLabelText, getByText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    const input = getByLabelText('Git refs') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'HEAD~3..HEAD' } });
    await tick();
    const run = getByText('Run review') as HTMLButtonElement;
    expect(run.disabled).toBe(false);
  });

  it('PR tab requires non-empty pr', async () => {
    vi.stubGlobal('fetch', makeFetch({ '/api/csrf-token': csrfHandler }).fn);
    const { component, getByRole, getByText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    const tab = getByRole('tab', { name: 'PR' });
    await fireEvent.click(tab);
    await tick();
    const run = getByText('Run review') as HTMLButtonElement;
    expect(run.disabled).toBe(true);
  });

  it('Staged tab is always valid', async () => {
    vi.stubGlobal('fetch', makeFetch({ '/api/csrf-token': csrfHandler }).fn);
    const { component, getByRole, getByText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    const tab = getByRole('tab', { name: 'Staged' });
    await fireEvent.click(tab);
    await tick();
    const run = getByText('Run review') as HTMLButtonElement;
    expect(run.disabled).toBe(false);
  });

  it('Paste tab requires diff text', async () => {
    vi.stubGlobal('fetch', makeFetch({ '/api/csrf-token': csrfHandler }).fn);
    const { component, getByRole, getByText, getByLabelText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    await fireEvent.click(getByRole('tab', { name: 'Paste diff' }));
    await tick();
    let run = getByText('Run review') as HTMLButtonElement;
    expect(run.disabled).toBe(true);
    const ta = getByLabelText('Diff text') as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'diff --git a/x b/x\n' } });
    await tick();
    run = getByText('Run review') as HTMLButtonElement;
    expect(run.disabled).toBe(false);
  });

  it('fetches preview with CSRF header', async () => {
    const { fn, calls } = makeFetch({
      '/api/csrf-token': csrfHandler,
      '/api/runs/preview': () =>
        new Response(JSON.stringify(previewBody), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
    });
    vi.stubGlobal('fetch', fn);
    const { component, getByLabelText, getByText, findByText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    await fireEvent.input(getByLabelText('Git refs') as HTMLInputElement, {
      target: { value: 'HEAD~1..HEAD' },
    });
    await tick();
    await fireEvent.click(getByText('Estimate cost'));

    // Wait for preview row
    await findByText('Group 0');

    const previewCall = calls.find((c) => c.url.includes('/api/runs/preview'));
    expect(previewCall).toBeDefined();
    const headers = (previewCall!.init?.headers ?? {}) as Record<string, string>;
    expect(headers['X-CSRF-Token']).toBe('test-csrf');
    const body = JSON.parse(previewCall!.init!.body as string);
    expect(body.mode).toBe('git');
    expect(body.git_args).toEqual(['HEAD~1..HEAD']);
  });

  it('submits run with CSRF header on Run review', async () => {
    const runResp: RunResponse = { id: 'newid' };
    const { fn, calls } = makeFetch({
      '/api/csrf-token': csrfHandler,
      '/api/runs': (() => {
        let first = true;
        return () => {
          // Distinguish /api/runs vs /api/runs/preview by URL — preview matches earlier.
          if (first) {
            first = false;
            return new Response(JSON.stringify(runResp), {
              status: 202,
              headers: { 'content-type': 'application/json' },
            });
          }
          return new Response(JSON.stringify(runResp), {
            status: 202,
            headers: { 'content-type': 'application/json' },
          });
        };
      })(),
    });
    vi.stubGlobal('fetch', fn);
    const { component, getByLabelText, getByText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    await fireEvent.input(getByLabelText('Git refs') as HTMLInputElement, {
      target: { value: 'main..feature' },
    });
    await tick();
    await fireEvent.click(getByText('Run review'));
    // Wait until the run POST has been issued.
    for (let i = 0; i < 20; i++) {
      await tick();
      if (calls.some((c) => /\/api\/runs(\?|$)/.test(c.url) && (c.init?.method ?? '').toUpperCase() === 'POST')) break;
    }

    const runCall = calls.find(
      (c) =>
        /\/api\/runs(\?|$)/.test(c.url) &&
        (c.init?.method ?? '').toUpperCase() === 'POST'
    );
    expect(runCall).toBeDefined();
    const headers = (runCall!.init?.headers ?? {}) as Record<string, string>;
    expect(headers['X-CSRF-Token']).toBe('test-csrf');
    const body = JSON.parse(runCall!.init!.body as string);
    expect(body.mode).toBe('git');
    expect(body.git_args).toEqual(['main..feature']);
  });

  it('shows preview error inline without blocking Run review', async () => {
    vi.stubGlobal(
      'fetch',
      makeFetch({
        '/api/csrf-token': csrfHandler,
        '/api/runs/preview': () => new Response('boom', { status: 500 }),
      }).fn
    );
    const { component, getByLabelText, getByText, findByText } = render(NewReviewDialog);
    (component as unknown as { show: () => Promise<void> }).show();
    await tick(); await tick();
    await fireEvent.input(getByLabelText('Git refs') as HTMLInputElement, {
      target: { value: 'HEAD~1..HEAD' },
    });
    await tick();
    await fireEvent.click(getByText('Estimate cost'));
    await findByText(/boom|Preview failed/i);
    const run = getByText('Run review') as HTMLButtonElement;
    expect(run.disabled).toBe(false);
  });
});

describe('splitGitArgs', () => {
  it('simple whitespace split', () => {
    expect(splitGitArgs('main feature HEAD~1')).toEqual(['main', 'feature', 'HEAD~1']);
  });

  it('preserves spaces inside double quotes', () => {
    expect(splitGitArgs('--grep "fix bug" main')).toEqual(['--grep', 'fix bug', 'main']);
  });

  it('preserves spaces inside single quotes', () => {
    expect(splitGitArgs("--author 'Alice Bob'")).toEqual(['--author', 'Alice Bob']);
  });

  it('handles backslash escapes', () => {
    expect(splitGitArgs('foo\\ bar baz')).toEqual(['foo bar', 'baz']);
  });

  it('trims trailing/leading whitespace', () => {
    expect(splitGitArgs('   HEAD~3..HEAD   ')).toEqual(['HEAD~3..HEAD']);
  });

  it('returns empty array for empty input', () => {
    expect(splitGitArgs('')).toEqual([]);
    expect(splitGitArgs('   ')).toEqual([]);
  });
});
