<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchResults } from '$lib/api';
  import type { ResultSummary } from '$lib/types';
  import RepoCard from '$lib/components/RepoCard.svelte';

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

  interface RepoGroup {
    name: string;
    results: ResultSummary[];
    latest: number;
  }

  $: groups = (() => {
    const byRepo = new Map<string, ResultSummary[]>();
    for (const r of results) {
      const key = r.repo_name ?? 'Unknown repo';
      const arr = byRepo.get(key) ?? [];
      arr.push(r);
      byRepo.set(key, arr);
    }
    const out: RepoGroup[] = [];
    for (const [name, arr] of byRepo) {
      arr.sort((a, b) => (a.created_at < b.created_at ? 1 : -1));
      const latest = new Date(arr[0]?.created_at ?? 0).getTime();
      out.push({ name, results: arr, latest });
    }
    out.sort((a, b) => b.latest - a.latest);
    return out;
  })();
</script>

<div class="container">
  <header>
    <div class="header-row">
      <div>
        <h1>semantic-diff</h1>
        <p class="subtitle">AI-powered semantic code reviews</p>
      </div>
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
    {#each groups as group (group.name)}
      <RepoCard repoName={group.name} results={group.results} />
    {/each}
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
</style>
