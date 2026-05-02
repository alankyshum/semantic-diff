<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fetchResults } from '$lib/api';
  import type { ResultSummary, ResultDocument } from '$lib/types';
  import RepoCard from '$lib/components/RepoCard.svelte';
  import NewReviewDialog from '$lib/components/NewReviewDialog.svelte';
  import CostChart from '$lib/components/CostChart.svelte';
  import { paletteItems, type PaletteItem } from '$lib/keyboard';
  import { mapWithConcurrency } from '$lib/util/concurrency';

  let results: ResultSummary[] = [];
  // Full ResultDocument list (lazily fetched) for the cost chart.
  let fullResults: ResultDocument[] = [];
  let loading = true;
  let error = '';
  let dialog: NewReviewDialog | null = null;
  let chartOpen = false;

  function openDialog() {
    dialog?.show();
  }

  async function loadFullResults() {
    // /api/results returns summaries only; we need metadata.tokens.cost_usd.
    // Fetch each /api/result/:id concurrently. This is fine for a 30-day chart;
    // most installs have <100 reviews.
    try {
      const docs = await mapWithConcurrency(results, 6, async (r) => {
        try {
          const res = await fetch(`/api/result/${r.id}`);
          if (!res.ok) return null;
          return (await res.json()) as ResultDocument;
        } catch { return null; }
      });
      fullResults = docs.filter((d): d is ResultDocument => d !== null);
    } catch {
      fullResults = [];
    }
  }

  let paletteHandle: { remove: () => void } | null = null;

  onMount(async () => {
    try {
      chartOpen = localStorage.getItem('home-cost-chart-open') === '1';
    } catch { /* ignore */ }

    try {
      results = await fetchResults();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }

    if (chartOpen && results.length > 0) {
      void loadFullResults();
    }

    const item: PaletteItem = {
      id: 'home:new-review',
      label: 'New review',
      group: 'Actions',
      action: () => openDialog(),
    };
    paletteItems.update((cur) => [...cur, item]);
    paletteHandle = {
      remove: () => paletteItems.update((cur) => cur.filter((i) => i.id !== item.id)),
    };
  });

  onDestroy(() => {
    paletteHandle?.remove();
    paletteHandle = null;
  });

  function toggleChart() {
    chartOpen = !chartOpen;
    try { localStorage.setItem('home-cost-chart-open', chartOpen ? '1' : '0'); } catch { /* ignore */ }
    if (chartOpen && fullResults.length === 0 && results.length > 0) {
      void loadFullResults();
    }
  }

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
      <div class="header-actions">
        <button type="button" class="new-btn" on:click={openDialog}>+ New Review</button>
      </div>
    </div>
  </header>

  <section class="cost-section">
    <button
      type="button"
      class="cost-toggle"
      aria-expanded={chartOpen}
      on:click={toggleChart}
    >
      <span class="caret" class:open={chartOpen} aria-hidden="true">▶</span>
      Cost (30 days)
    </button>
    {#if chartOpen}
      <div class="cost-body">
        <CostChart results={fullResults} />
      </div>
    {/if}
  </section>

  {#if loading}
    <div class="loading">Loading reviews…</div>
  {:else if error}
    <div class="error">Error: {error}</div>
  {:else if results.length === 0}
    <div class="empty">
      <p>No reviews yet.</p>
      <p>Click <strong>+ New Review</strong> above, or run <code>semantic-diff</code> on a diff.</p>
    </div>
  {:else}
    {#each groups as group (group.name)}
      <RepoCard repoName={group.name} results={group.results} />
    {/each}
  {/if}
</div>

<NewReviewDialog bind:this={dialog} />

<style>
  .container {
    max-width: var(--content-max);
    margin: 0 auto;
    padding: 2rem clamp(1rem, 4vw, 3rem);
  }
  header { margin-bottom: 1.5rem; }
  .header-row { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; }
  h1 { font-size: 1.8rem; margin: 0 0 0.25rem; }
  .subtitle { color: var(--color-fg-muted); margin: 0; }
  .header-actions { display: flex; gap: 0.5rem; align-items: center; }
  .new-btn {
    background: var(--color-accent);
    color: var(--color-bg);
    border: 1px solid var(--color-accent);
    border-radius: 6px;
    padding: 0.5rem 1rem;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
  }
  .new-btn:hover { filter: brightness(1.1); }

  .cost-section {
    margin-bottom: 1.5rem;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-elev);
  }
  .cost-toggle {
    width: 100%;
    background: transparent;
    border: none;
    color: var(--color-fg);
    text-align: left;
    padding: 0.6rem 1rem;
    font-size: 0.85rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .cost-toggle:hover { background: var(--color-bg-inset); }
  .caret { transition: transform 0.15s; display: inline-block; font-size: 0.7rem; color: var(--color-fg-muted); }
  .caret.open { transform: rotate(90deg); }
  .cost-body { padding: 0.5rem 1rem 1rem; border-top: 1px solid var(--color-border); }

  .loading, .empty { color: var(--color-fg-muted); text-align: center; padding: 3rem; }
  .error { color: var(--color-danger); padding: 1rem; border: 1px solid var(--color-danger); border-radius: 6px; }
</style>
