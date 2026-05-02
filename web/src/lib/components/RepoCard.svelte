<script lang="ts">
  import type { ResultSummary } from '$lib/types';
  import { formatDate, statusColor } from '$lib/util/date';

  export let repoName: string;
  export let remoteUrl: string | undefined = undefined;
  export let results: ResultSummary[];
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
</style>
