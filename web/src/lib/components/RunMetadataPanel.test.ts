import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import RunMetadataPanel from './RunMetadataPanel.svelte';
import type { RunMetadata, RepoInfo } from '$lib/types';

const metadata: RunMetadata = {
  tool_version: '0.2.0',
  schema_version: 2,
  started_at: '2025-01-01T00:00:00Z',
  completed_at: '2025-01-01T00:01:30Z',
  cli_argv: ['semantic-diff', 'review', '--diff', 'foo.patch'],
  working_dir: '/tmp/work',
  llm: { provider: 'anthropic', model: 'claude-3', cli_path: '/usr/bin/claude', cli_version: '1.0' },
  timings: [
    { group_id: 'g0', section: 'WHY', duration_ms: 1200, cache_hit: false },
    { group_id: 'g1', section: 'WHY', duration_ms: 800, cache_hit: true },
    { group_id: 'g0', section: 'VERDICT', duration_ms: 2000, cache_hit: false },
  ],
  total_duration_ms: 4500,
  skill_files: [{ name: 'review-cm', path: '/skills/x.md', hash_blake3: 'abcdef0123456789' }],
  tokens: { input_tokens: 1000, output_tokens: 500, cost_usd: 0.0123 },
};

const repo: RepoInfo = { name: 'demo', branch: 'main', head_sha: 'abcdef1234567', remote_url: 'https://x.test/r' };

describe('RunMetadataPanel', () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn(() => Promise.resolve()) },
    });
  });

  it('renders all field categories', () => {
    const { getByText, container } = render(RunMetadataPanel, { props: { metadata, repo } });
    // Tool version
    expect(getByText('0.2.0')).toBeTruthy();
    // LLM
    expect(getByText('anthropic')).toBeTruthy();
    expect(getByText('claude-3')).toBeTruthy();
    // Repo - "demo" appears as repo name
    expect(getByText('demo')).toBeTruthy();
    expect(getByText('main')).toBeTruthy();
    // Tokens header
    expect(getByText('Tokens')).toBeTruthy();
    // Timings table — section names
    expect(getByText('Per-section timings')).toBeTruthy();
    expect(container.querySelector('table.timings')).toBeTruthy();
    // Skill files
    expect(getByText('review-cm')).toBeTruthy();
  });

  it('copy button calls navigator.clipboard.writeText', async () => {
    const { getByLabelText } = render(RunMetadataPanel, { props: { metadata, repo } });
    const btn = getByLabelText('Copy CLI argv');
    await fireEvent.click(btn);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('semantic-diff review --diff foo.patch');
  });
});
