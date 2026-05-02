<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { RawConfig, AiCli, LlmProviderName } from '$lib/types';

  /** Bound config object. The form mutates a local copy and emits `change` with sanitized values. */
  export let value: RawConfig;

  const dispatch = createEventDispatcher<{ change: RawConfig }>();

  const VALID_PROVIDERS: readonly LlmProviderName[] = ['claude', 'copilot', 'cursor'] as const;

  // Local input strings — avoid binding directly to value.* so we can normalize ""→null on emit.
  let preferred: '' | AiCli = (value['preferred-ai-cli'] ?? '') as '' | AiCli;
  let providersText: string = providersToText(value['llm-providers']);
  let providersError = '';
  let claudeModel: string = value.claude.model ?? '';
  let copilotModel: string = value.copilot.model ?? '';
  let cursorModel: string = value.cursor.model ?? '';

  // Re-sync local fields when parent replaces `value` (e.g. Reset / Discard / re-fetch).
  // Compare a stable signature so ordinary user typing doesn't trigger resets.
  let lastValueSig = JSON.stringify(value);
  $: {
    const sig = JSON.stringify(value);
    if (sig !== lastValueSig) {
      lastValueSig = sig;
      preferred = (value['preferred-ai-cli'] ?? '') as '' | AiCli;
      providersText = providersToText(value['llm-providers']);
      providersError = '';
      claudeModel = value.claude.model ?? '';
      copilotModel = value.copilot.model ?? '';
      cursorModel = value.cursor.model ?? '';
    }
  }

  function providersToText(arr: LlmProviderName[] | null | undefined): string {
    if (!arr || arr.length === 0) return '';
    return arr.join('\n');
  }

  function parseProviders(text: string): { ok: true; value: LlmProviderName[] | null } | { ok: false; error: string } {
    const lines = text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter((l) => l.length > 0);
    if (lines.length === 0) return { ok: true, value: null };
    const bad = lines.filter((l) => !VALID_PROVIDERS.includes(l as LlmProviderName));
    if (bad.length > 0) {
      return { ok: false, error: `Invalid provider(s): ${bad.join(', ')}. Allowed: ${VALID_PROVIDERS.join(', ')}.` };
    }
    return { ok: true, value: lines as LlmProviderName[] };
  }

  function emit() {
    if (providersError) return; // don't emit while invalid
    const parsed = parseProviders(providersText);
    if (!parsed.ok) return;
    const next: RawConfig = {
      'preferred-ai-cli': preferred === '' ? null : preferred,
      'llm-providers': parsed.value,
      claude: { model: claudeModel.trim() === '' ? null : claudeModel },
      copilot: { model: copilotModel.trim() === '' ? null : copilotModel },
      cursor: { model: cursorModel.trim() === '' ? null : cursorModel },
    };
    // keep our snapshot in sync so the value-watcher doesn't bounce.
    lastValueSig = JSON.stringify(next);
    dispatch('change', next);
  }

  function onProvidersBlur() {
    const parsed = parseProviders(providersText);
    if (!parsed.ok) {
      providersError = parsed.error;
    } else {
      providersError = '';
      emit();
    }
  }

  function onProvidersInput() {
    // Clear inline error as soon as user edits; full validation happens on blur.
    if (providersError) {
      const parsed = parseProviders(providersText);
      if (parsed.ok) providersError = '';
    }
  }
</script>

<div class="config-form">
  <div class="field">
    <label for="cf-preferred">Preferred AI CLI</label>
    <select id="cf-preferred" bind:value={preferred} on:change={emit}>
      <option value="">Auto (use first available)</option>
      <option value="claude">Claude</option>
      <option value="copilot">Copilot</option>
    </select>
    <p class="help">Forces a specific CLI when multiple are installed. Leave on "Auto" to use the order below.</p>
  </div>

  <div class="field">
    <label for="cf-providers">LLM provider order</label>
    <textarea
      id="cf-providers"
      rows="3"
      spellcheck="false"
      bind:value={providersText}
      on:input={onProvidersInput}
      on:blur={onProvidersBlur}
      placeholder="claude&#10;copilot&#10;cursor"
      aria-invalid={providersError !== ''}
      aria-describedby={providersError ? 'cf-providers-error' : 'cf-providers-help'}
    ></textarea>
    {#if providersError}
      <p class="error" id="cf-providers-error" role="alert">{providersError}</p>
    {/if}
    <p class="help" id="cf-providers-help">
      One provider per line. Order matters; first available wins. Empty = use default order: claude, copilot, cursor.
    </p>
  </div>

  <div class="field">
    <label for="cf-claude-model">Claude model</label>
    <input id="cf-claude-model" type="text" bind:value={claudeModel} on:change={emit} placeholder="default" autocomplete="off" />
    <p class="help">Override the model used by the Claude CLI (e.g. <code>claude-sonnet-4-5</code>). Empty = CLI default.</p>
  </div>

  <div class="field">
    <label for="cf-copilot-model">Copilot model</label>
    <input id="cf-copilot-model" type="text" bind:value={copilotModel} on:change={emit} placeholder="default" autocomplete="off" />
    <p class="help">Override the model used by the Copilot CLI. Empty = CLI default.</p>
  </div>

  <div class="field">
    <label for="cf-cursor-model">Cursor model</label>
    <input id="cf-cursor-model" type="text" bind:value={cursorModel} on:change={emit} placeholder="default" autocomplete="off" />
    <p class="help">Override the model used by the Cursor CLI. Empty = CLI default.</p>
  </div>
</div>

<style>
  .config-form { display: flex; flex-direction: column; gap: 1.1rem; }
  .field { display: flex; flex-direction: column; gap: 0.3rem; }
  label {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--color-fg);
  }
  input[type='text'], select, textarea {
    background: var(--color-bg-elev);
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    border-radius: 6px;
    padding: 0.45rem 0.6rem;
    font-size: 0.9rem;
    font-family: inherit;
    width: 100%;
  }
  textarea {
    font-family: 'Fira Code', 'Cascadia Code', monospace;
    font-size: 0.85rem;
    resize: vertical;
    min-height: 4.5rem;
  }
  input[type='text']:focus, select:focus, textarea:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent);
  }
  .help {
    font-size: 0.75rem;
    color: var(--color-fg-muted);
    margin: 0;
    line-height: 1.4;
  }
  .help code {
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0 0.3rem;
    font-size: 0.7rem;
  }
  .error {
    color: var(--color-danger);
    font-size: 0.78rem;
    margin: 0;
  }
  textarea[aria-invalid='true'] {
    border-color: var(--color-danger);
  }
</style>
