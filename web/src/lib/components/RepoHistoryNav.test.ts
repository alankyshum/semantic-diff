import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import RepoHistoryNav from './RepoHistoryNav.svelte';
import type { ResultSummary } from '$lib/types';

const threeFixture: ResultSummary[] = [
  { id: 'cur', title: 'Current review', created_at: '2025-03-01T00:00:00Z', status: 'complete', repo_name: 'demo' },
  { id: 'old1', title: 'Older review one', created_at: '2025-02-01T00:00:00Z', status: 'complete', repo_name: 'demo' },
  { id: 'old2', title: 'Older review two', created_at: '2025-01-01T00:00:00Z', status: 'failed', repo_name: 'demo' },
];

const oneFixture: ResultSummary[] = [
  { id: 'cur', title: 'Lonely', created_at: '2025-03-01T00:00:00Z', status: 'complete', repo_name: 'demo' },
];

function okFetch(payload: unknown) {
  return vi.fn(() =>
    Promise.resolve({
      ok: true,
      status: 200,
      json: () => Promise.resolve(payload),
    } as Response),
  );
}

function failFetch() {
  return vi.fn(() => Promise.reject(new Error('network down')));
}

describe('RepoHistoryNav', () => {
  let originalFetch: typeof globalThis.fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it('shows History (3) after fetch returns 3 summaries', async () => {
    globalThis.fetch = okFetch(threeFixture);
    const { getByRole } = render(RepoHistoryNav, {
      props: { repoName: 'demo', currentId: 'cur' },
    });
    const btn = getByRole('button');
    await waitFor(() => expect(btn.textContent).toContain('History (3)'));
    expect(btn.hasAttribute('disabled')).toBe(false);
  });

  it('disables the button when only one result exists', async () => {
    globalThis.fetch = okFetch(oneFixture);
    const { getByRole } = render(RepoHistoryNav, {
      props: { repoName: 'demo', currentId: 'cur' },
    });
    const btn = getByRole('button') as HTMLButtonElement;
    await waitFor(() => expect(btn.textContent).toContain('History (1)'));
    expect(btn.disabled).toBe(true);
  });

  it('opens a dropdown with one row per item when clicked', async () => {
    globalThis.fetch = okFetch(threeFixture);
    const { getByRole, container } = render(RepoHistoryNav, {
      props: { repoName: 'demo', currentId: 'cur' },
    });
    const btn = getByRole('button');
    await waitFor(() => expect(btn.textContent).toContain('History (3)'));

    await fireEvent.click(btn);
    await waitFor(() => expect(container.querySelector('.panel')).toBeTruthy());
    const rows = container.querySelectorAll('.row');
    expect(rows.length).toBe(3);
    expect(btn.getAttribute('aria-expanded')).toBe('true');
    expect(container.querySelector('[role="menu"]')).toBeTruthy();
  });

  it('renders the current row as a non-link with aria-current="true"', async () => {
    globalThis.fetch = okFetch(threeFixture);
    const { getByRole, container } = render(RepoHistoryNav, {
      props: { repoName: 'demo', currentId: 'cur' },
    });
    await waitFor(() => expect(getByRole('button').textContent).toContain('History (3)'));
    await fireEvent.click(getByRole('button'));
    await waitFor(() => expect(container.querySelector('.panel')).toBeTruthy());

    const current = container.querySelector('.row.current') as HTMLElement;
    expect(current).toBeTruthy();
    expect(current.tagName.toLowerCase()).toBe('div');
    expect(current.getAttribute('aria-current')).toBe('true');
  });

  it('renders other rows as <a href="/r/${id}">', async () => {
    globalThis.fetch = okFetch(threeFixture);
    const { getByRole, container } = render(RepoHistoryNav, {
      props: { repoName: 'demo', currentId: 'cur' },
    });
    await waitFor(() => expect(getByRole('button').textContent).toContain('History (3)'));
    await fireEvent.click(getByRole('button'));
    await waitFor(() => expect(container.querySelector('.panel')).toBeTruthy());

    const anchors = container.querySelectorAll('a.row');
    expect(anchors.length).toBe(2);
    const hrefs = Array.from(anchors).map((a) => a.getAttribute('href'));
    expect(hrefs).toContain('/r/old1');
    expect(hrefs).toContain('/r/old2');
  });

  it('closes the dropdown when a click happens outside', async () => {
    globalThis.fetch = okFetch(threeFixture);
    const { getByRole, container } = render(RepoHistoryNav, {
      props: { repoName: 'demo', currentId: 'cur' },
    });
    await waitFor(() => expect(getByRole('button').textContent).toContain('History (3)'));
    await fireEvent.click(getByRole('button'));
    await waitFor(() => expect(container.querySelector('.panel')).toBeTruthy());

    const outside = document.createElement('div');
    document.body.appendChild(outside);
    await fireEvent.mouseDown(outside);
    await waitFor(() => expect(container.querySelector('.panel')).toBeNull());
    document.body.removeChild(outside);
  });

  it('shows History (?) and the panel surfaces the error when fetch fails', async () => {
    globalThis.fetch = failFetch();
    const { getByRole, container } = render(RepoHistoryNav, {
      props: { repoName: 'demo', currentId: 'cur' },
    });
    const btn = getByRole('button') as HTMLButtonElement;
    await waitFor(() => expect(btn.textContent).toContain('History (?)'));
    // The button is clickable in error state so the user can read the error.
    expect(btn.disabled).toBe(false);

    await fireEvent.click(btn);
    await waitFor(() => expect(container.querySelector('.panel')).toBeTruthy());
    const errEl = container.querySelector('.panel .error');
    expect(errEl).toBeTruthy();
    expect(errEl?.textContent).toMatch(/network down/);
  });
});
