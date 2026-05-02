<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { RawConfig, AiCli, LlmProvider } from '$lib/types';
  import { defaultRawConfig } from '$lib/types';

  /** Initial config the form starts with and "Reset" reverts to. */
  export let initial: RawConfig;
  /** Disable Save while a parent-driven request is in flight. */
  export let saving = false;

  const dispatch = createEventDispatcher<{ save: RawConfig }>();

  const ALL_PROVIDERS = ['claude', 'copilot', 'cursor'] as const satisfies readonly LlmProvider[];
  // Compile-time check: every member of LlmProvider must be in ALL_PROVIDERS.
  type _ExhaustiveCheck = Exclude<LlmProvider, typeof ALL_PROVIDERS[number]> extends never ? true : never;
  const _exhaustiveCheckRef: _ExhaustiveCheck = true;
  void _exhaustiveCheckRef;

  // Local form state — clone-from-initial; never mutate the prop.
  let preferred: '' | AiCli = '';
  let providerOrder: LlmProvider[] = [];
  let claudeModel = '';
  let copilotModel = '';
  let cursorModel = '';

  // For the "Add provider" select: the user picks from this and we apply it on change.
  let addProviderChoice = '';

  // Track the snapshot we last hydrated from so external changes to `initial`
  // (after Save → re-fetch, or parent reset) re-sync local state.
  let lastInitialSig = '';
  $: hydrate(initial);

  function hydrate(cfg: RawConfig) {
    const sig = JSON.stringify(cfg);
    if (sig === lastInitialSig) return;
    lastInitialSig = sig;
    const merged: RawConfig = { ...defaultRawConfig(), ...cfg };
    merged.claude = { ...defaultRawConfig().claude, ...(cfg?.claude ?? {}) };
    merged.copilot = { ...defaultRawConfig().copilot, ...(cfg?.copilot ?? {}) };
    merged.cursor = { ...defaultRawConfig().cursor, ...(cfg?.cursor ?? {}) };

    preferred = (merged['preferred-ai-cli'] ?? '') as '' | AiCli;
    providerOrder = [...(merged['llm-providers'] ?? [])];
    claudeModel = merged.claude.model ?? '';
    copilotModel = merged.copilot.model ?? '';
    cursorModel = merged.cursor.model ?? '';
    addProviderChoice = '';
  }

  /** Build the RawConfig that this form would emit, normalizing empty strings → null. */
  function buildPayload(): RawConfig {
    return {
      'preferred-ai-cli': preferred === '' ? null : preferred,
      'llm-providers': providerOrder.length === 0 ? null : [...providerOrder],
      claude: { model: claudeModel.trim() === '' ? null : claudeModel.trim() },
      copilot: { model: copilotModel.trim() === '' ? null : copilotModel.trim() },
      cursor: { model: cursorModel.trim() === '' ? null : cursorModel.trim() },
    };
  }

  /** Canonical signature of `initial` after the same null-coalescing rules buildPayload uses. */
  function canonicalInitial(): RawConfig {
    return {
      'preferred-ai-cli': initial['preferred-ai-cli'] ?? null,
      'llm-providers':
        initial['llm-providers'] && initial['llm-providers'].length > 0
          ? [...initial['llm-providers']]
          : null,
      claude: { model: initial.claude?.model ?? null },
      copilot: { model: initial.copilot?.model ?? null },
      cursor: { model: initial.cursor?.model ?? null },
    };
  }

  // Reactively recompute current payload + dirty state on every input change.
  // Reference each tracked variable so Svelte invalidates this block on any change.
  $: current = ((): RawConfig => {
    void preferred;
    void providerOrder;
    void claudeModel;
    void copilotModel;
    void cursorModel;
    return buildPayload();
  })();
  $: dirty = JSON.stringify(current) !== JSON.stringify(canonicalInitial());

  // Available providers for the "Add" select = full list minus ones already chosen.
  $: availableToAdd = ALL_PROVIDERS.filter((p) => !providerOrder.includes(p));

  function moveUp(idx: number) {
    if (idx <= 0) return;
    const next = [...providerOrder];
    [next[idx - 1], next[idx]] = [next[idx], next[idx - 1]];
    providerOrder = next;
  }
  function moveDown(idx: number) {
    if (idx >= providerOrder.length - 1) return;
    const next = [...providerOrder];
    [next[idx], next[idx + 1]] = [next[idx + 1], next[idx]];
    providerOrder = next;
  }
  function remove(idx: number) {
    providerOrder = providerOrder.filter((_, i) => i !== idx);
  }
  function addProvider() {
    if (!addProviderChoice) return;
    if (!ALL_PROVIDERS.includes(addProviderChoice as LlmProvider)) return;
    if (providerOrder.includes(addProviderChoice as LlmProvider)) return;
    providerOrder = [...providerOrder, addProviderChoice as LlmProvider];
    addProviderChoice = '';
  }

  function reset() {
    // Force re-hydrate even if `initial` reference is unchanged.
    lastInitialSig = '';
    hydrate(initial);
  }

  function onSubmit(e: Event) {
    e.preventDefault();
    if (!dirty || saving) return;
    dispatch('save', buildPayload());
  }
</script>

<form class="settings-form" on:submit={onSubmit} novalidate>
  <div class="field">
    <label for="sf-preferred">Preferred AI CLI</label>
    <select id="sf-preferred" bind:value={preferred}>
      <option value="">(default — first available)</option>
      <option value="claude">claude</option>
      <option value="copilot">copilot</option>
    </select>
    <p class="help">
      Forces a specific CLI when more than one is installed. Leave on default to use the provider order
      below.
    </p>
  </div>

  <div class="field">
    <span class="label-text" id="sf-providers-label">LLM provider order</span>
    <div class="chips" role="list" aria-labelledby="sf-providers-label">
      {#if providerOrder.length === 0}
        <p class="muted chips-empty">No order set — built-in default is used (claude, copilot, cursor).</p>
      {:else}
        {#each providerOrder as p, idx (p)}
          <div class="chip" role="listitem" data-provider={p}>
            <span class="chip-label">{p}</span>
            <div class="chip-actions">
              <button
                type="button"
                class="chip-btn"
                aria-label="Move {p} up"
                title="Move up"
                disabled={idx === 0}
                on:click={() => moveUp(idx)}
              >
                ↑
              </button>
              <button
                type="button"
                class="chip-btn"
                aria-label="Move {p} down"
                title="Move down"
                disabled={idx === providerOrder.length - 1}
                on:click={() => moveDown(idx)}
              >
                ↓
              </button>
              <button
                type="button"
                class="chip-btn chip-remove"
                aria-label="Remove {p}"
                title="Remove"
                on:click={() => remove(idx)}
              >
                ×
              </button>
            </div>
          </div>
        {/each}
      {/if}
    </div>

    {#if availableToAdd.length > 0}
      <div class="add-row">
        <label for="sf-add-provider" class="add-label">Add provider</label>
        <select id="sf-add-provider" bind:value={addProviderChoice}>
          <option value="">— select —</option>
          {#each availableToAdd as p (p)}
            <option value={p}>{p}</option>
          {/each}
        </select>
        <button
          type="button"
          class="btn btn-small"
          on:click={addProvider}
          disabled={!addProviderChoice}
        >
          Add
        </button>
      </div>
    {/if}

    <p class="help">
      Order matters — first available wins. Empty list serializes as <code>null</code> so the
      built-in default order is used.
    </p>
  </div>

  <div class="field">
    <label for="sf-claude-model">Claude model</label>
    <input
      id="sf-claude-model"
      type="text"
      bind:value={claudeModel}
      placeholder="(built-in default)"
      autocomplete="off"
      spellcheck="false"
    />
    <p class="help">
      Override the model used by the Claude CLI (e.g. <code>claude-sonnet-4-5</code>). Empty = CLI
      default.
    </p>
  </div>

  <div class="field">
    <label for="sf-copilot-model">Copilot model</label>
    <input
      id="sf-copilot-model"
      type="text"
      bind:value={copilotModel}
      placeholder="(built-in default)"
      autocomplete="off"
      spellcheck="false"
    />
    <p class="help">Override the model used by the Copilot CLI. Empty = CLI default.</p>
  </div>

  <div class="field">
    <label for="sf-cursor-model">Cursor model</label>
    <input
      id="sf-cursor-model"
      type="text"
      bind:value={cursorModel}
      placeholder="(built-in default)"
      autocomplete="off"
      spellcheck="false"
    />
    <p class="help">Override the model used by the Cursor CLI. Empty = CLI default.</p>
  </div>

  <div class="actions">
    <button type="submit" class="btn btn-primary" disabled={!dirty || saving}>
      {saving ? 'Saving…' : 'Save'}
    </button>
    <button type="button" class="btn" on:click={reset} disabled={!dirty || saving}>
      Reset
    </button>
  </div>
</form>

<style>
  .settings-form {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  label,
  .label-text {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--color-fg);
  }
  input[type='text'],
  select {
    background: var(--color-bg-elev);
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    border-radius: 6px;
    padding: 0.45rem 0.6rem;
    font-size: 0.9rem;
    font-family: inherit;
    width: 100%;
  }
  input[type='text']:focus,
  select:focus {
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
  .muted {
    color: var(--color-fg-muted);
    font-size: 0.8rem;
    margin: 0;
  }

  /* Provider chips */
  .chips {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.4rem;
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    min-height: 2.5rem;
  }
  .chips-empty {
    margin: 0.25rem 0.25rem;
  }
  .chip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.35rem 0.55rem;
    background: var(--color-bg-elev);
    border: 1px solid var(--color-border);
    border-radius: 6px;
  }
  .chip-label {
    font-family: 'Fira Code', 'Cascadia Code', monospace;
    font-size: 0.85rem;
    color: var(--color-fg);
  }
  .chip-actions {
    display: inline-flex;
    gap: 0.25rem;
  }
  .chip-btn {
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    border-radius: 4px;
    width: 1.6rem;
    height: 1.6rem;
    font-size: 0.85rem;
    line-height: 1;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .chip-btn:hover:not(:disabled) {
    background: var(--color-bg-elev);
    border-color: var(--color-accent);
  }
  .chip-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }
  .chip-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .chip-remove:hover:not(:disabled) {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }

  .add-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .add-label {
    font-size: 0.78rem;
    font-weight: 500;
    color: var(--color-fg-muted);
  }
  .add-row select {
    width: auto;
    min-width: 9rem;
    flex: 0 0 auto;
  }

  /* Action row */
  .actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-top: 0.5rem;
    padding-top: 1rem;
    border-top: 1px solid var(--color-border);
  }
  .btn {
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    border-radius: 6px;
    padding: 0.45rem 0.9rem;
    font-size: 0.85rem;
    cursor: pointer;
    font-family: inherit;
  }
  .btn:hover:not(:disabled) {
    background: var(--color-bg-elev);
    border-color: var(--color-accent);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-small {
    padding: 0.3rem 0.7rem;
    font-size: 0.78rem;
  }
  .btn-primary {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-bg);
    font-weight: 600;
  }
  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }
</style>
