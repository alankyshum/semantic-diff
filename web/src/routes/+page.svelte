<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchResults } from '$lib/api';
  import type { ResultSummary } from '$lib/types';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';

  let results: ResultSummary[] = [];
  let loading = true;
  let error = '';

  onMount(async () => {
    try {
      results = await fetchResults();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function statusColor(status: string): string {
    switch (status) {
      case 'complete': return 'var(--color-success)';
      case 'running': return 'var(--color-warning)';
      case 'failed': return 'var(--color-danger)';
      default: return 'var(--color-fg-muted)';
    }
  }

  function formatDate(iso: string): string {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }
</script>

<div class="container">
  <header>
    <div class="header-row">
      <div>
        <h1>semantic-diff</h1>
        <p class="subtitle">AI-powered semantic code reviews</p>
      </div>
      <ThemeToggle />
    </div>
  </header>

  {#if loading}
    <div class="loading">Loading reviews…</div>
  {:else if error}
    <div class="error">Error: {error}</div>
  {:else if results.length === 0}
    <div class="empty">
      <p>No reviews yet.</p>
      <p>Run <code>semantic-diff</code> on a diff to create one.</p>
    </div>
  {:else}
    <div class="results-list">
      {#each results as result}
        <a href="/r/{result.id}" class="result-card">
          <div class="result-header">
            <span class="result-title">{result.title || result.id}</span>
            <span class="result-status" style="color: {statusColor(result.status)}">
              {result.status}
            </span>
          </div>
          <div class="result-meta">
            <span class="result-id">{result.id}</span>
            <span class="result-date">{formatDate(result.created_at)}</span>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .container {
    max-width: var(--content-max);
    margin: 0 auto;
    padding: 2rem clamp(1rem, 4vw, 3rem);
  }
  header { margin-bottom: 2rem; }
  .header-row { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; }
  h1 { font-size: 1.8rem; margin: 0 0 0.25rem; }
  .subtitle { color: var(--color-fg-muted); margin: 0; }
  .loading, .empty { color: var(--color-fg-muted); text-align: center; padding: 3rem; }
  .error { color: var(--color-danger); padding: 1rem; border: 1px solid var(--color-danger); border-radius: 6px; }
  .results-list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
    gap: 0.75rem;
  }
  .result-card {
    display: block;
    padding: 1rem 1.25rem;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-elev);
    transition: border-color 0.15s;
  }
  .result-card:hover { border-color: var(--color-accent); text-decoration: none; }
  .result-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.4rem; }
  .result-title { font-weight: 600; color: var(--color-fg); }
  .result-status { font-size: 0.75rem; font-weight: 600; text-transform: uppercase; }
  .result-meta { display: flex; gap: 1rem; font-size: 0.8rem; color: var(--color-fg-muted); }
</style>
