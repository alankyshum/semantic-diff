<script lang="ts" context="module">
  // Module-level CSRF token cache so multiple components share the same token.
  let _csrfToken: string | null = null;
  let _csrfFetchInflight: Promise<string> | null = null;

  export async function getCsrfToken(): Promise<string> {
    if (_csrfToken) return _csrfToken;
    if (_csrfFetchInflight) return _csrfFetchInflight;
    _csrfFetchInflight = (async () => {
      const res = await fetch('/api/csrf-token');
      if (!res.ok) throw new Error(`Failed to fetch CSRF token: ${res.status}`);
      const body = (await res.json()) as { token: string };
      _csrfToken = body.token;
      return _csrfToken;
    })();
    try {
      return await _csrfFetchInflight;
    } finally {
      _csrfFetchInflight = null;
    }
  }

  /** Test-only reset. */
  export function __resetCsrfToken(): void {
    _csrfToken = null;
    _csrfFetchInflight = null;
  }

  /**
   * Tokenize git-args input with shell-like quoting. Supports double and
   * single quotes (no nesting), backslash escapes, and whitespace separation.
   * Exported for unit tests.
   */
  export function splitGitArgs(input: string): string[] {
    const tokens: string[] = [];
    let cur = '';
    let quote: '"' | "'" | null = null;
    let escaped = false;
    for (const ch of input.trim()) {
      if (escaped) { cur += ch; escaped = false; continue; }
      if (ch === '\\') { escaped = true; continue; }
      if (quote) {
        if (ch === quote) { quote = null; continue; }
        cur += ch;
      } else if (ch === '"' || ch === "'") {
        quote = ch;
      } else if (/\s/.test(ch)) {
        if (cur) { tokens.push(cur); cur = ''; }
      } else {
        cur += ch;
      }
    }
    if (cur) tokens.push(cur);
    return tokens;
  }
</script>

<script lang="ts">
  // F11: NewReviewDialog — modal for kicking off a new review run.
  // F20: includes inline cost-preview area + per-section opt-out checkboxes.
  import { onDestroy, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import type { RunRequest, RunResponse, PreviewResponse } from '$lib/types';

  type Mode = 'git' | 'pr' | 'paste' | 'staged';
  const SECTIONS = ['WHY', 'WHAT', 'HOW', 'VERDICT'] as const;

  let dialogEl: HTMLDialogElement | null = null;
  let firstInputEl: HTMLInputElement | HTMLTextAreaElement | null = null;
  export let open = false;

  let mode: Mode = 'git';
  let gitArgs = '';
  let pr = '';
  let diffText = '';
  let workingDir = '';
  let title = '';
  let noLlm = false;

  /** `${groupId}::${section}` keys that have been opted out. */
  let skipped = new Set<string>();

  let preview: PreviewResponse | null = null;
  let previewLoading = false;
  let previewError = '';

  let submitting = false;
  let submitError = '';

  function buildRequest(): RunRequest {
    const skip_sections: Array<[string, string]> = Array.from(skipped).map((k) => {
      const idx = k.indexOf('::');
      return [k.slice(0, idx), k.slice(idx + 2)];
    });
    const req: RunRequest = { mode };
    if (title.trim()) req.title = title.trim();
    if (noLlm) req.no_llm = true;
    if (skip_sections.length > 0) req.skip_sections = skip_sections;
    if (mode === 'git') req.git_args = splitGitArgs(gitArgs);
    if (mode === 'pr') req.pr = pr.trim();
    if (mode === 'paste') {
      req.diff_text = diffText;
      if (workingDir.trim()) req.working_dir = workingDir.trim();
    }
    return req;
  }

  $: formValid = (() => {
    if (mode === 'git') return splitGitArgs(gitArgs).length > 0;
    if (mode === 'pr') return pr.trim().length > 0;
    if (mode === 'paste') return diffText.length > 0;
    return true; // staged
  })();

  // W5: invalidate any cached preview whenever inputs that affect the request
  // change. Keep the explicit "Estimate cost" button as the trigger — we don't
  // auto-fetch on every keystroke (per existing UX).
  $: { void mode; void gitArgs; void pr; void diffText; void noLlm; preview = null; previewError = ''; }

  function sectionSkipped(groupId: string, section: string): boolean {
    return skipped.has(`${groupId}::${section}`);
  }

  function toggleSkip(groupId: string, section: string) {
    const key = `${groupId}::${section}`;
    const next = new Set(skipped);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    skipped = next;
    // Re-fetch preview so totals update, but don't block the UI.
    void runPreview();
  }

  async function runPreview() {
    if (!formValid) return;
    previewLoading = true;
    previewError = '';
    try {
      const token = await getCsrfToken();
      const res = await fetch('/api/runs/preview', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': token },
        body: JSON.stringify(buildRequest()),
      });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new Error(text || `Preview failed: ${res.status}`);
      }
      preview = (await res.json()) as PreviewResponse;
    } catch (e) {
      preview = null;
      previewError = e instanceof Error ? e.message : String(e);
    } finally {
      previewLoading = false;
    }
  }

  async function runReview() {
    if (!formValid || submitting) return;
    submitting = true;
    submitError = '';
    try {
      const token = await getCsrfToken();
      const res = await fetch('/api/runs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': token },
        body: JSON.stringify(buildRequest()),
      });
      if (res.status !== 202) {
        const text = await res.text().catch(() => '');
        throw new Error(text || `Run failed: ${res.status}`);
      }
      const body = (await res.json()) as RunResponse;
      closeDialog();
      void goto(`/r/${body.id}`);
    } catch (e) {
      submitError = e instanceof Error ? e.message : String(e);
    } finally {
      submitting = false;
    }
  }

  function setMode(m: Mode) {
    mode = m;
    // Clear preview because it's now stale.
    preview = null;
    previewError = '';
    // Refocus first input.
    void tick().then(() => firstInputEl?.focus());
  }

  export async function show() {
    open = true;
    preview = null;
    previewError = '';
    submitError = '';
    skipped = new Set();
    await tick();
    if (dialogEl && !dialogEl.open) {
      try { dialogEl.showModal(); } catch { /* jsdom fallback */ }
    }
    firstInputEl?.focus();
  }

  export function hide() {
    closeDialog();
  }

  function closeDialog() {
    if (dialogEl?.open) {
      try { dialogEl.close(); } catch { /* ignore */ }
    }
    open = false;
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) closeDialog();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeDialog();
    }
  }

  function fmtUsd(n: number | undefined | null): string {
    if (typeof n !== 'number' || !isFinite(n)) return '$0.00';
    if (n < 0.01 && n > 0) return '<$0.01';
    return `$${n.toFixed(2)}`;
  }

  function fmtTok(n: number | undefined | null): string {
    if (typeof n !== 'number' || !isFinite(n)) return '0';
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
    return String(n);
  }

  /** Sum tokens/cost across non-skipped sections only. */
  $: previewVisible = (() => {
    if (!preview) return null;
    let inTok = 0;
    let outTok = 0;
    let cost = 0;
    const groupRows = preview.groups.map((g) => {
      let gIn = 0;
      let gOut = 0;
      let gCost = 0;
      const sections: Record<string, { input: number; output: number; cost: number; skipped: boolean }> = {};
      for (const [section, ps] of Object.entries(g.sections)) {
        const skipFlag = sectionSkipped(g.group_id, section);
        sections[section] = {
          input: ps.input_tokens,
          output: ps.output_tokens_est,
          cost: ps.cost_usd,
          skipped: skipFlag,
        };
        if (!skipFlag) {
          gIn += ps.input_tokens;
          gOut += ps.output_tokens_est;
          gCost += ps.cost_usd;
        }
      }
      inTok += gIn;
      outTok += gOut;
      cost += gCost;
      return { group_id: g.group_id, title: g.title, sections, gIn, gOut, gCost };
    });
    return { groupRows, inTok, outTok, cost };
  })();

  onDestroy(() => {
    if (dialogEl?.open) {
      try { dialogEl.close(); } catch { /* ignore */ }
    }
  });
</script>

<dialog
  bind:this={dialogEl}
  class="newrev"
  on:click={onBackdropClick}
  on:keydown={onKeydown}
  on:close={() => { open = false; }}
  aria-label="New review"
>
  <div class="inner" role="document">
    <header class="hd">
      <h2>New review</h2>
      <button type="button" class="close" aria-label="Close" on:click={closeDialog}>×</button>
    </header>

    <div class="tablist" role="tablist" aria-label="Source mode">
      {#each ['git', 'pr', 'paste', 'staged'] as const as m}
        <button
          type="button"
          role="tab"
          class="tab"
          class:active={mode === m}
          aria-selected={mode === m}
          on:click={() => setMode(m)}
        >{m === 'git' ? 'Git refs' : m === 'pr' ? 'PR' : m === 'paste' ? 'Paste diff' : 'Staged'}</button>
      {/each}
    </div>

    <div class="form">
      {#if mode === 'git'}
        <label class="field">
          <span>Git refs</span>
          <input
            bind:this={firstInputEl}
            type="text"
            bind:value={gitArgs}
            placeholder="HEAD~3..HEAD or main..feature-branch"
            aria-label="Git refs"
          />
          <small>Comma- or space-separated. Passed to <code>git diff</code>.</small>
        </label>
      {:else if mode === 'pr'}
        <label class="field">
          <span>PR</span>
          <input
            bind:this={firstInputEl}
            type="text"
            bind:value={pr}
            placeholder="owner/repo#42 or full GitHub URL"
            aria-label="PR"
          />
        </label>
      {:else if mode === 'paste'}
        <label class="field">
          <span>Unified diff</span>
          <textarea
            bind:this={firstInputEl}
            bind:value={diffText}
            placeholder="Paste unified diff here…"
            rows="8"
            aria-label="Diff text"
          ></textarea>
        </label>
        <label class="field">
          <span>Working directory (optional)</span>
          <input type="text" bind:value={workingDir} placeholder="defaults to current" aria-label="Working directory" />
        </label>
      {:else}
        <p class="hint">Will review currently staged changes (<code>git diff --cached</code>).</p>
      {/if}

      <label class="field">
        <span>Title (optional)</span>
        <input type="text" bind:value={title} aria-label="Title" />
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={noLlm} />
        <span>Skip LLM (for grouping/cache only)</span>
      </label>
    </div>

    <section class="cost">
      <div class="cost-head">
        <h3>Cost estimate</h3>
        <button
          type="button"
          class="btn"
          on:click={runPreview}
          disabled={!formValid || previewLoading}
        >{previewLoading ? 'Estimating…' : 'Estimate cost'}</button>
      </div>
      {#if previewError}
        <div class="err" role="alert">{previewError}</div>
      {/if}
      {#if preview?.degraded}
        <div class="warn" role="alert">
          ⚠ Estimate may be inaccurate — semantic grouping fell back to a single bucket{preview.degraded_reason ? `: ${preview.degraded_reason}` : ''}
        </div>
      {/if}
      {#if previewVisible}
        <table class="cost-table">
          <thead>
            <tr>
              <th>Group</th>
              {#each SECTIONS as s}<th>{s}</th>{/each}
              <th class="num">Tokens</th>
              <th class="num">Cost</th>
            </tr>
          </thead>
          <tbody>
            {#each previewVisible.groupRows as row (row.group_id)}
              <tr>
                <td title={row.group_id}>{row.title}</td>
                {#each SECTIONS as section}
                  {@const ps = row.sections[section]}
                  <td class="cell">
                    {#if ps}
                      <label class="cb" title="Include {section}">
                        <input
                          type="checkbox"
                          checked={!ps.skipped}
                          on:change={() => toggleSkip(row.group_id, section)}
                          aria-label="Include {section} in {row.title}"
                        />
                      </label>
                    {:else}
                      <span class="dash">—</span>
                    {/if}
                  </td>
                {/each}
                <td class="num">{fmtTok(row.gIn + row.gOut)}</td>
                <td class="num">{fmtUsd(row.gCost)}</td>
              </tr>
            {/each}
          </tbody>
          <tfoot>
            <tr>
              <td colspan={SECTIONS.length + 1}><strong>Total</strong></td>
              <td class="num"><strong>{fmtTok(previewVisible.inTok + previewVisible.outTok)}</strong></td>
              <td class="num"><strong>{fmtUsd(previewVisible.cost)}</strong></td>
            </tr>
          </tfoot>
        </table>
      {/if}
    </section>

    {#if submitError}
      <div class="err" role="alert">{submitError}</div>
    {/if}

    <footer class="actions">
      <button type="button" class="btn" on:click={closeDialog}>Cancel</button>
      <button
        type="button"
        class="btn primary"
        disabled={!formValid || submitting}
        on:click={runReview}
      >{submitting ? 'Starting…' : 'Run review'}</button>
    </footer>
  </div>
</dialog>

<style>
  dialog.newrev {
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-elev);
    color: var(--color-fg);
    padding: 0;
    width: min(760px, 95vw);
    max-height: 90vh;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.4);
  }
  dialog.newrev::backdrop { background: rgba(0, 0, 0, 0.45); }
  .inner {
    display: flex;
    flex-direction: column;
    max-height: 90vh;
    overflow: hidden;
  }
  .hd {
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.85rem 1.1rem;
    border-bottom: 1px solid var(--color-border);
  }
  .hd h2 { margin: 0; font-size: 1.05rem; }
  .close {
    background: transparent; border: none; color: var(--color-fg-muted);
    font-size: 1.4rem; line-height: 1; cursor: pointer; padding: 0 0.3rem;
  }
  .close:hover { color: var(--color-fg); }

  .tablist {
    display: flex;
    border-bottom: 1px solid var(--color-border);
    padding: 0 0.5rem;
    flex-shrink: 0;
  }
  .tab {
    background: transparent;
    border: none;
    color: var(--color-fg-muted);
    padding: 0.55rem 0.9rem;
    font-size: 0.85rem;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tab:hover { color: var(--color-fg); }
  .tab.active {
    color: var(--color-fg);
    border-bottom-color: var(--color-accent);
    font-weight: 600;
  }

  .form {
    padding: 0.9rem 1.1rem;
    display: flex; flex-direction: column; gap: 0.7rem;
    overflow-y: auto;
  }
  .field { display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.85rem; }
  .field > span { color: var(--color-fg-muted); font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .field input, .field textarea {
    background: var(--color-bg);
    color: var(--color-fg);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    padding: 0.4rem 0.55rem;
    font-size: 0.88rem;
    font-family: inherit;
  }
  .field textarea { font-family: 'Fira Code', 'Cascadia Code', monospace; resize: vertical; }
  .field small { color: var(--color-fg-muted); font-size: 0.72rem; }
  .check { display: flex; align-items: center; gap: 0.4rem; font-size: 0.85rem; color: var(--color-fg); }
  .hint { color: var(--color-fg-muted); margin: 0; font-size: 0.85rem; }

  .cost {
    border-top: 1px solid var(--color-border);
    padding: 0.85rem 1.1rem;
    overflow-y: auto;
  }
  .cost-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem; }
  .cost-head h3 { margin: 0; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--color-fg-muted); }

  .cost-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.82rem;
  }
  .cost-table th, .cost-table td {
    padding: 0.35rem 0.4rem;
    border-bottom: 1px solid var(--color-border);
    text-align: left;
  }
  .cost-table th { color: var(--color-fg-muted); font-weight: 600; font-size: 0.74rem; text-transform: uppercase; }
  .cost-table .num { text-align: right; font-variant-numeric: tabular-nums; }
  .cost-table .cell { text-align: center; }
  .cost-table tfoot td { border-bottom: none; padding-top: 0.5rem; }
  .cb { display: inline-flex; cursor: pointer; }
  .dash { color: var(--color-fg-muted); }

  .err {
    color: var(--color-danger);
    background: var(--color-bg-inset);
    border: 1px solid var(--color-danger);
    border-radius: 5px;
    padding: 0.45rem 0.65rem;
    margin: 0.5rem 1.1rem;
    font-size: 0.82rem;
  }

  .warn {
    color: var(--color-warning);
    background: var(--color-bg-inset);
    border: 1px solid var(--color-warning);
    border-radius: 5px;
    padding: 0.45rem 0.65rem;
    margin: 0.5rem 0;
    font-size: 0.82rem;
  }

  .actions {
    display: flex; justify-content: flex-end; gap: 0.5rem;
    border-top: 1px solid var(--color-border);
    padding: 0.7rem 1.1rem;
    flex-shrink: 0;
  }
  .btn {
    background: var(--color-bg);
    color: var(--color-fg);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    padding: 0.4rem 0.85rem;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) { background: var(--color-bg-inset); border-color: var(--color-accent); }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn.primary {
    background: var(--color-accent);
    color: var(--color-bg);
    border-color: var(--color-accent);
    font-weight: 600;
  }
  .btn.primary:hover:not(:disabled) { filter: brightness(1.1); }
</style>
