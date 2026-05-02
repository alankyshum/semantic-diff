import { describe, it, expect } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ConfigForm from './ConfigForm.svelte';
import type { RawConfig } from '$lib/types';
import { defaultRawConfig } from '$lib/types';

function makeValue(overrides: Partial<RawConfig> = {}): RawConfig {
  return { ...defaultRawConfig(), ...overrides };
}

/**
 * Helper: render ConfigForm and capture all 'change' events into an array.
 * Uses Svelte 5's `events` mount option (deprecated but functional) since
 * `component.$on(...)` is removed in Svelte 5.
 */
function renderWithEvents(value: RawConfig) {
  const events: RawConfig[] = [];
  const result = render(ConfigForm, {
    props: { value },
    events: { change: (e: CustomEvent<RawConfig>) => events.push(e.detail) },
  });
  return { ...result, events };
}

describe('ConfigForm', () => {
  it('renders all 5 fields with prefilled value', () => {
    const value: RawConfig = {
      'preferred-ai-cli': 'claude',
      'llm-providers': ['copilot', 'claude'],
      claude: { model: 'claude-sonnet-4-5' },
      copilot: { model: 'gpt-4o' },
      cursor: { model: 'cursor-fast' },
    };
    const { getByLabelText } = render(ConfigForm, { props: { value } });

    const preferred = getByLabelText('Preferred AI CLI') as HTMLSelectElement;
    expect(preferred.value).toBe('claude');

    const providers = getByLabelText('LLM provider order') as HTMLTextAreaElement;
    expect(providers.value).toBe('copilot\nclaude');

    expect((getByLabelText('Claude model') as HTMLInputElement).value).toBe('claude-sonnet-4-5');
    expect((getByLabelText('Copilot model') as HTMLInputElement).value).toBe('gpt-4o');
    expect((getByLabelText('Cursor model') as HTMLInputElement).value).toBe('cursor-fast');
  });

  it('emits model: null for empty model inputs on change', async () => {
    const value = makeValue({ claude: { model: 'old' } });
    const { getByLabelText, events } = renderWithEvents(value);

    const claudeInput = getByLabelText('Claude model') as HTMLInputElement;
    await fireEvent.input(claudeInput, { target: { value: '' } });
    await fireEvent.change(claudeInput);

    expect(events.length).toBeGreaterThan(0);
    const last = events[events.length - 1];
    expect(last.claude.model).toBeNull();
    expect(last.copilot.model).toBeNull();
    expect(last.cursor.model).toBeNull();
  });

  it('emits llm-providers: null when textarea is empty', async () => {
    const value = makeValue({ 'llm-providers': ['claude', 'copilot'] });
    const { getByLabelText, events } = renderWithEvents(value);

    const providers = getByLabelText('LLM provider order') as HTMLTextAreaElement;
    await fireEvent.input(providers, { target: { value: '' } });
    await fireEvent.blur(providers);

    expect(events.length).toBeGreaterThan(0);
    const last = events[events.length - 1];
    expect(last['llm-providers']).toBeNull();
  });

  it('shows inline error and suppresses change emit until invalid provider is corrected', async () => {
    const value = makeValue();
    const { getByLabelText, events, queryByRole } = renderWithEvents(value);

    const providers = getByLabelText('LLM provider order') as HTMLTextAreaElement;
    await fireEvent.input(providers, { target: { value: 'bogus' } });
    await fireEvent.blur(providers);

    // inline error visible, no emit on blur
    const alert = queryByRole('alert');
    expect(alert).not.toBeNull();
    expect(alert!.textContent).toMatch(/Invalid provider/i);
    expect(events.length).toBe(0);

    // Even forcing a model change while the error is present must NOT emit
    // (otherwise we'd overwrite RawConfig with a stale providers list).
    const claudeInput = getByLabelText('Claude model') as HTMLInputElement;
    await fireEvent.input(claudeInput, { target: { value: 'foo' } });
    await fireEvent.change(claudeInput);
    expect(events.length).toBe(0);

    // Correcting the error and blurring should clear error and emit.
    await fireEvent.input(providers, { target: { value: 'claude\ncopilot' } });
    await fireEvent.blur(providers);
    expect(queryByRole('alert')).toBeNull();
    expect(events.length).toBeGreaterThan(0);
    const last = events[events.length - 1];
    expect(last['llm-providers']).toEqual(['claude', 'copilot']);
  });
});
