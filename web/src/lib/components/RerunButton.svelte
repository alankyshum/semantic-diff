<script lang="ts">
  // F11: RerunButton — small ↻ icon to rerun a single section.
  import { getCsrfToken } from './NewReviewDialog.svelte';

  export let resultId: string;
  export let groupId: string;
  export let section: 'why' | 'what' | 'how' | 'verdict';

  let inflight = false;
  let cooldown = false;
  let error = '';

  async function rerun() {
    if (inflight || cooldown) return;
    error = '';
    inflight = true;
    try {
      const token = await getCsrfToken();
      const url = `/api/runs/${encodeURIComponent(resultId)}/sections/${encodeURIComponent(groupId)}/${encodeURIComponent(section)}/rerun`;
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'X-CSRF-Token': token },
      });
      if (res.status !== 202) {
        const text = await res.text().catch(() => '');
        throw new Error(text || `Rerun failed: ${res.status}`);
      }
      // SSE will push the loading→ready transition. Briefly disable to prevent spam.
      cooldown = true;
      setTimeout(() => { cooldown = false; }, 3000);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      inflight = false;
    }
  }
</script>

<button
  type="button"
  class="rerun"
  on:click={rerun}
  disabled={inflight || cooldown}
  title={error || 'Re-run this section'}
  aria-label="Re-run {section}"
>
  {#if inflight}
    <span class="spin" aria-hidden="true">⟳</span>
  {:else}
    <span aria-hidden="true">↻</span>
  {/if}
</button>

<style>
  .rerun {
    background: transparent;
    border: 1px solid var(--color-border);
    color: var(--color-fg-muted);
    border-radius: 4px;
    width: 22px;
    height: 22px;
    line-height: 1;
    font-size: 0.85rem;
    cursor: pointer;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .rerun:hover:not(:disabled) {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-bg-inset);
  }
  .rerun:disabled { opacity: 0.5; cursor: not-allowed; }
  .spin {
    display: inline-block;
    animation: spin 1s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
