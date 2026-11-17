<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { getCsrfToken } from './NewReviewDialog.svelte';
  import type { ResultSummary } from '$lib/types';
  import { formatDate, statusColor } from '$lib/util/date';

  export let repoName: string;
  export let remoteUrl: string | undefined = undefined;
  export let results: ResultSummary[];

  const dispatch = createEventDispatcher<{ deleted: string }>();
  let deleteError = '';

  async function remove(e: Event, result: ResultSummary) {
    e.preventDefault();
    e.stopPropagation();
    if (!confirm(`Delete review "${result.title || result.id}"? This cannot be undone.`)) return;
    deleteError = '';
    try {
      const token = await getCsrfToken();
      const res = await fetch(`/api/result/${encodeURIComponent(result.id)}`, {
        method: 'DELETE',
        headers: { 'X-CSRF-Token': token },
      });
      if (!res.ok && res.status !== 404) {
        throw new Error((await res.text().catch(() => '')) || `Delete failed: ${res.status}`);
      }
      dispatch('deleted', result.id);
    } catch (err) {
      deleteError = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<section class="repo-card" aria-label={repoName}>
  <header class="repo-header">
    <h2 class="repo-name">{repoName}</h2>
    {#if remoteUrl}
      <a class="repo-link" href={remoteUrl} target="_blank" rel="noopener noreferrer">
        {remoteUrl} <span aria-hidden="true">↗</span>
      </a>
    {/if}
    <span class="repo-count">{results.length} review{results.length === 1 ? '' : 's'}</span>
  </header>

  {#if deleteError}<div class="delete-error" role="alert">{deleteError}</div>{/if}
  <div class="repo-grid">
    {#each results as result (result.id)}
      <a class="mini-card" href="/r/{result.id}">
        <div class="mini-header">
          <span class="mini-title">{result.title || result.id}</span>
          <span class="mini-status" style="color: {statusColor(result.status)}">{result.status}</span>
        </div>
        <div class="mini-meta">
          <span class="mini-date">{formatDate(result.created_at)}</span>
          <span class="mini-id">{result.id}</span>
          <button
            type="button"
            class="mini-delete"
            title="Delete review"
            aria-label="Delete review {result.id}"
            on:click={(e) => remove(e, result)}
          >Delete</button>
        </div>
      </a>
    {/each}
  </div>
</section>

<style>
  .repo-card {
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 1rem 1.25rem;
    background: var(--color-bg-elev);
    margin-bottom: 1.25rem;
  }
  .repo-header {
    display: flex; align-items: baseline; flex-wrap: wrap; gap: 0.75rem;
    margin-bottom: 0.75rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--color-border);
  }
  .repo-name { margin: 0; font-size: 1.05rem; }
  .repo-link {
    font-size: 0.8rem; color: var(--color-fg-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .repo-count {
    margin-left: auto; font-size: 0.75rem; color: var(--color-fg-muted);
  }
  .repo-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 0.5rem;
  }
  .mini-card {
    display: block;
    padding: 0.6rem 0.8rem;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg);
    transition: border-color 0.15s;
  }
  .mini-card:hover { border-color: var(--color-accent); text-decoration: none; }
  .mini-header { display: flex; justify-content: space-between; align-items: center; gap: 0.5rem; margin-bottom: 0.25rem; }
  .mini-title { font-weight: 600; color: var(--color-fg); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mini-status { font-size: 0.7rem; font-weight: 600; text-transform: uppercase; flex-shrink: 0; }
  .mini-meta { display: flex; gap: 0.75rem; font-size: 0.75rem; color: var(--color-fg-muted); }
  .mini-id { font-family: monospace; }
  .mini-delete {
    margin-left: auto; background: transparent; border: 1px solid var(--color-border);
    border-radius: 4px; color: var(--color-fg-muted); font-size: 0.7rem;
    padding: 0 0.4rem; cursor: pointer;
  }
  .mini-delete:hover { color: var(--color-danger); border-color: var(--color-danger); }
  .delete-error { color: var(--color-danger); font-size: 0.8rem; margin-bottom: 0.5rem; }
</style>
