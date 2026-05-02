import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import RepoCard from './RepoCard.svelte';
import type { ResultSummary } from '$lib/types';

const fixture: ResultSummary[] = [
  { id: 'abc', title: 'First review', created_at: '2025-01-01T00:00:00Z', status: 'complete', repo_name: 'demo' },
  { id: 'def', title: 'Second review', created_at: '2025-02-01T00:00:00Z', status: 'running', repo_name: 'demo' },
];

describe('RepoCard', () => {
  it('renders repo header and N child mini-cards', () => {
    const { getByText, getAllByRole, container } = render(RepoCard, {
      props: { repoName: 'demo', results: fixture },
    });
    expect(getByText('demo')).toBeTruthy();
    // 2 mini-card links
    const links = getAllByRole('link');
    expect(links.length).toBe(2);
    expect(links[0].getAttribute('href')).toBe('/r/abc');
    expect(container.querySelector('section[aria-label="demo"]')).toBeTruthy();
  });

  it('renders remote link when remoteUrl provided', () => {
    const { getByText } = render(RepoCard, {
      props: { repoName: 'demo', remoteUrl: 'https://example.com/x', results: fixture },
    });
    expect(getByText(/example\.com/)).toBeTruthy();
  });
});
