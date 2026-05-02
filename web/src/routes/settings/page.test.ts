import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import Page from './+page.svelte';
import type { ConfigEnvelope, ProbeReport } from '$lib/types';

const envelope: ConfigEnvelope = {
  path: '/home/user/.config/semantic-diff/config.json',
  exists: true,
  raw: {
    'preferred-ai-cli': 'claude',
    'llm-providers': ['claude', 'copilot'],
    claude: { model: null },
    copilot: { model: null },
    cursor: { model: null },
  },
};

const probe: ProbeReport = {
  providers: [
    { name: 'claude', binaries: [{ name: 'claude', found: true, path: '/usr/bin/claude', version: '1.2.3' }] },
    { name: 'copilot', binaries: [{ name: 'gh', found: false, path: null, version: null }] },
  ],
};

function mockFetch() {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === 'string' ? input : input.toString();
    if (url.endsWith('/api/config')) {
      return new Response(JSON.stringify(envelope), { status: 200, headers: { 'content-type': 'application/json' } });
    }
    if (url.endsWith('/api/config/probe')) {
      return new Response(JSON.stringify(probe), { status: 200, headers: { 'content-type': 'application/json' } });
    }
    return new Response('not found', { status: 404 });
  });
}

beforeEach(() => {
  vi.stubGlobal('fetch', mockFetch());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('Settings page', () => {
  it('renders form and provider panel after data resolves', async () => {
    const { getByRole, getByLabelText, getByText, findByText } = render(Page);

    // Heading is present immediately
    expect(getByRole('heading', { level: 1, name: /Settings/i })).toBeTruthy();

    // Wait for the config path to appear (signals config fetch resolved)
    await findByText('/home/user/.config/semantic-diff/config.json');

    // Form fields rendered with prefilled value
    await waitFor(() => {
      const sel = getByLabelText('Preferred AI CLI') as HTMLSelectElement;
      expect(sel.value).toBe('claude');
    });

    // Provider panel rendered both providers
    await waitFor(() => {
      // 'claude' appears as both the provider-name and the bin-name, so use getAllByText.
      expect(getByText('claude', { selector: '.provider-name' })).toBeTruthy();
      expect(getByText('copilot', { selector: '.provider-name' })).toBeTruthy();
    });
  });
});
