<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchResults } from '$lib/api';
  import type { ResultSummary } from '$lib/types';

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
      case 'complete': return '#3fb950';
      case 'running': return '#d29922';
      case 'failed': return '#f85149';
      default: return '#8b949e';
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
    <h1>semantic-diff</h1>
    <p class="subtitle">AI-powered semantic code reviews</p>
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
    max-width: 800px;
    margin: 0 auto;
    padding: 2rem 1rem;
  }
  header { margin-bottom: 2rem; }
  h1 { font-size: 1.8rem; margin: 0 0 0.25rem; }
  .subtitle { color: #8b949e; margin: 0; }
  .loading, .empty { color: #8b949e; text-align: center; padding: 3rem; }
  .error { color: #f85149; padding: 1rem; border: 1px solid #f85149; border-radius: 6px; }
  .results-list { display: flex; flex-direction: column; gap: 0.75rem; }
  .result-card {
    display: block;
    padding: 1rem 1.25rem;
    border: 1px solid #30363d;
    border-radius: 8px;
    background: #161b22;
    transition: border-color 0.15s;
  }
  .result-card:hover { border-color: #58a6ff; text-decoration: none; }
  .result-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.4rem; }
  .result-title { font-weight: 600; color: #e6edf3; }
  .result-status { font-size: 0.75rem; font-weight: 600; text-transform: uppercase; }
  .result-meta { display: flex; gap: 1rem; font-size: 0.8rem; color: #8b949e; }
</style>
