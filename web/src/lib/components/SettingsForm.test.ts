import { describe, it, expect } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import SettingsForm from './SettingsForm.svelte';
import type { RawConfig } from '$lib/types';
import { defaultRawConfig } from '$lib/types';

function makeInitial(overrides: Partial<RawConfig> = {}): RawConfig {
  return { ...defaultRawConfig(), ...overrides };
}

/**
 * Render SettingsForm and capture all 'save' events into an array.
 * Mirrors the pattern used by ConfigForm.test.ts (Svelte 5 events option).
 */
function renderForm(initial: RawConfig) {
  const events: RawConfig[] = [];
  const result = render(SettingsForm, {
    props: { initial },
    events: { save: (e: CustomEvent<RawConfig>) => events.push(e.detail) },
  });
  return { ...result, events };
}

describe('SettingsForm', () => {
  it('renders with initial values populated in inputs', () => {
    const initial: RawConfig = {
      'preferred-ai-cli': 'claude',
      'llm-providers': ['copilot', 'claude'],
      claude: { model: 'claude-sonnet-4-5' },
      copilot: { model: 'gpt-4o' },
      cursor: { model: 'cursor-fast' },
    };
    const { getByLabelText, getAllByRole } = render(SettingsForm, { props: { initial } });

    const preferred = getByLabelText('Preferred AI CLI') as HTMLSelectElement;
    expect(preferred.value).toBe('claude');

    expect((getByLabelText('Claude model') as HTMLInputElement).value).toBe('claude-sonnet-4-5');
    expect((getByLabelText('Copilot model') as HTMLInputElement).value).toBe('gpt-4o');
    expect((getByLabelText('Cursor model') as HTMLInputElement).value).toBe('cursor-fast');

    // Provider chips render in declared order.
    const items = getAllByRole('listitem');
    expect(items.map((el) => el.getAttribute('data-provider'))).toEqual(['copilot', 'claude']);
  });

  it('Save button disabled when no changes', () => {
    const { getByRole } = render(SettingsForm, { props: { initial: makeInitial() } });
    const save = getByRole('button', { name: /^Save$/ }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
  });

  it('editing a model field enables Save and dispatches save event with updated RawConfig', async () => {
    const initial = makeInitial({ claude: { model: 'old' } });
    const { getByLabelText, getByRole, events } = renderForm(initial);

    const claudeInput = getByLabelText('Claude model') as HTMLInputElement;
    await fireEvent.input(claudeInput, { target: { value: 'claude-sonnet-4-5' } });

    const save = getByRole('button', { name: /^Save$/ }) as HTMLButtonElement;
    expect(save.disabled).toBe(false);

    await fireEvent.click(save);

    expect(events.length).toBe(1);
    expect(events[0].claude.model).toBe('claude-sonnet-4-5');
    // unchanged fields remain null per default
    expect(events[0].copilot.model).toBeNull();
    expect(events[0].cursor.model).toBeNull();
    expect(events[0]['preferred-ai-cli']).toBeNull();
    expect(events[0]['llm-providers']).toBeNull();
  });

  it('empty model input string serializes as null in dispatched payload', async () => {
    const initial = makeInitial({
      claude: { model: 'old-claude' },
      copilot: { model: 'old-copilot' },
    });
    const { getByLabelText, getByRole, events } = renderForm(initial);

    const claudeInput = getByLabelText('Claude model') as HTMLInputElement;
    const copilotInput = getByLabelText('Copilot model') as HTMLInputElement;
    await fireEvent.input(claudeInput, { target: { value: '' } });
    await fireEvent.input(copilotInput, { target: { value: '   ' } }); // whitespace → also null

    const save = getByRole('button', { name: /^Save$/ }) as HTMLButtonElement;
    expect(save.disabled).toBe(false);
    await fireEvent.click(save);

    expect(events.length).toBe(1);
    expect(events[0].claude.model).toBeNull();
    expect(events[0].copilot.model).toBeNull();
  });

  it('Reset reverts changes and disables Save again', async () => {
    const initial = makeInitial({ claude: { model: 'original' } });
    const { getByLabelText, getByRole } = render(SettingsForm, { props: { initial } });

    const claudeInput = getByLabelText('Claude model') as HTMLInputElement;
    await fireEvent.input(claudeInput, { target: { value: 'changed' } });
    const save = getByRole('button', { name: /^Save$/ }) as HTMLButtonElement;
    expect(save.disabled).toBe(false);

    const reset = getByRole('button', { name: /^Reset$/ }) as HTMLButtonElement;
    await fireEvent.click(reset);

    // Reset reverts the field…
    expect((getByLabelText('Claude model') as HTMLInputElement).value).toBe('original');
    // …and disables Save.
    expect(save.disabled).toBe(true);
  });

  it('provider reorder: move cursor up; payload order matches', async () => {
    const initial = makeInitial({ 'llm-providers': ['claude', 'copilot', 'cursor'] });
    const { getByRole, getAllByRole, events } = renderForm(initial);

    // Move "cursor" (idx 2) up once → ['claude', 'cursor', 'copilot']
    const upCursor = getByRole('button', { name: /Move cursor up/ }) as HTMLButtonElement;
    expect(upCursor.disabled).toBe(false);
    await fireEvent.click(upCursor);

    // DOM order check
    const items = getAllByRole('listitem');
    expect(items.map((el) => el.getAttribute('data-provider'))).toEqual([
      'claude',
      'cursor',
      'copilot',
    ]);

    // Save and verify dispatched payload
    const save = getByRole('button', { name: /^Save$/ }) as HTMLButtonElement;
    expect(save.disabled).toBe(false);
    await fireEvent.click(save);

    expect(events.length).toBe(1);
    expect(events[0]['llm-providers']).toEqual(['claude', 'cursor', 'copilot']);
  });
});
