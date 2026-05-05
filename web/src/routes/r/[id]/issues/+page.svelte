<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { fetchResult } from '$lib/api';
  import type { ResultDocument, Issue, Severity } from '$lib/types';
  import SeverityBadge from '$lib/components/SeverityBadge.svelte';
  import MarkdownView from '$lib/components/MarkdownView.svelte';

  const ALL_SEVS: Severity[] = ['critical', 'high', 'medium', 'low', 'nit', 'info'];
  const SEV_RANK: Record<Severity, number> = {
    critical: 0, high: 1, medium: 2, low: 3, nit: 4, info: 5,
  };

  let doc: ResultDocument | null = null;
  let loading = true;
  let error = '';

  let sevFilter = new Set<Severity>(ALL_SEVS);
  let fileFilter = '';
  let groupFilter = '';

  $: resultId = $page.params.id as string;

  // Read filters from URL on mount.
  function readUrl() {
    const sp = $page.url.searchParams;
    const sev = sp.get('severity');
    if (sev) {
      // Accept any-case URL values (e.g. ?severity=Critical,high) and normalize
      // to canonical lowercase before validating against ALL_SEVS.
      const parts = sev.split(',').map(s => s.trim().toLowerCase()).filter(Boolean);
      sevFilter = new Set(parts.filter((p): p is Severity => (ALL_SEVS as string[]).includes(p)));
    }
    fileFilter = sp.get('file') ?? '';
    groupFilter = sp.get('group') ?? '';
  }

  function writeUrl() {
    const sp = new URLSearchParams();
    if (sevFilter.size > 0 && sevFilter.size < ALL_SEVS.length) {
      sp.set('severity', Array.from(sevFilter).join(','));
    }
    if (fileFilter) sp.set('file', fileFilter);
    if (groupFilter) sp.set('group', groupFilter);
    const qs = sp.toString();
    const target = qs ? `${location.pathname}?${qs}` : location.pathname;
    history.replaceState(history.state, '', target);
  }

  function toggleSev(s: Severity) {
    if (sevFilter.has(s)) sevFilter.delete(s); else sevFilter.add(s);
    sevFilter = new Set(sevFilter);
    writeUrl();
  }

  onMount(async () => {
    readUrl();
    try {
      doc = await fetchResult(resultId);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  interface FlatIssue extends Issue { groupId: string; groupLabel: string; }

  $: allIssues = doc
    ? Object.entries(doc.reviews).flatMap(([gid, r]) => {
        const g = doc!.groups.find(g => g.id === gid);
        return (r.verdict_issues ?? []).map(i => ({
          ...i,
          groupId: gid,
          groupLabel: g?.label ?? gid,
        } satisfies FlatIssue));
      })
    : [];

  $: groupOrder = doc ? doc.groups.map(g => g.id) : [];

  $: filtered = allIssues
    .filter(i => sevFilter.has(i.severity))
    .filter(i => !fileFilter || i.files.some(f => f.includes(fileFilter)))
    .filter(i => !groupFilter || i.groupId === groupFilter)
    .sort((a, b) => {
      const sa = SEV_RANK[a.severity] ?? 99;
      const sb = SEV_RANK[b.severity] ?? 99;
      if (sa !== sb) return sa - sb;
      const ga = groupOrder.indexOf(a.groupId);
      const gb = groupOrder.indexOf(b.groupId);
      return ga - gb;
    });
</script>

<div class="container">
  <header class="header">
    <div class="header-left">
      <a class="back" href="/r/{resultId}">← Back to review</a>
      {#if doc}<h1>Issues — {doc.title}</h1>{:else}<h1>Issues</h1>{/if}
    </div>
  </header>

  {#if loading}
    <div class="loading">Loading issues…</div>
  {:else if error}
    <div class="error">Error: {error}</div>
  {:else if doc}
    <div class="filters">
      <div class="filter-group">
        <span class="filter-label">Severity:</span>
        {#each ALL_SEVS as s}
          <label class="sev-check">
            <input type="checkbox" checked={sevFilter.has(s)} on:change={() => toggleSev(s)} />
            <SeverityBadge severity={s} />
          </label>
        {/each}
      </div>
      <div class="filter-group">
        <label class="filter-label" for="file-filter">File:</label>
        <input
          id="file-filter"
          type="text"
          placeholder="substring match"
          bind:value={fileFilter}
          on:input={writeUrl}
        />
      </div>
      <div class="filter-group">
        <label class="filter-label" for="group-filter">Group:</label>
        <select id="group-filter" bind:value={groupFilter} on:change={writeUrl}>
          <option value="">All groups</option>
          {#each doc.groups as g}
            <option value={g.id}>{g.label}</option>
          {/each}
        </select>
      </div>
    </div>

    {#if filtered.length === 0}
      <div class="empty">No issues match the current filters.</div>
    {:else}
      <div class="issues">
        {#each filtered as issue (issue.groupId + ':' + issue.id)}
          <article class="issue">
            <header class="issue-header">
              <SeverityBadge severity={issue.severity} />
              <span class="issue-id">{issue.id}</span>
              <h3 class="issue-title">{issue.title}</h3>
              <a class="issue-group" href="/r/{resultId}#issue-{issue.id}">{issue.groupLabel}</a>
            </header>
            <MarkdownView content={issue.body_md} />
            {#if issue.files.length}
              <footer class="issue-files">
                {#each issue.files as f}<code class="file-chip">{f}</code>{/each}
              </footer>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .container {
    max-width: var(--content-max);
    margin: 0 auto;
    padding: 1.5rem clamp(1rem, 4vw, 3rem);
  }
  .header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 1.25rem; gap: 1rem; flex-wrap: wrap; }
  .header-left { display: flex; flex-direction: column; gap: 0.25rem; }
  .back { color: var(--color-fg-muted); font-size: 0.85rem; }
  h1 { margin: 0; font-size: 1.4rem; }

  .loading, .empty { color: var(--color-fg-muted); text-align: center; padding: 3rem; }
  .error { color: var(--color-danger); padding: 1rem; border: 1px solid var(--color-danger); border-radius: 6px; }

  .filters {
    display: flex; flex-wrap: wrap; gap: 1.25rem; align-items: center;
    padding: 0.75rem 1rem;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-elev);
    margin-bottom: 1rem;
  }
  .filter-group { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
  .filter-label { font-size: 0.8rem; color: var(--color-fg-muted); }
  .sev-check { display: inline-flex; align-items: center; gap: 0.3rem; cursor: pointer; }
  .sev-check input { cursor: pointer; }
  input[type="text"], select {
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0.25rem 0.5rem;
    color: var(--color-fg);
    font-size: 0.85rem;
  }

  .issues { display: flex; flex-direction: column; gap: 0.75rem; }
  .issue {
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 1rem 1.25rem;
    background: var(--color-bg-elev);
  }
  .issue-header { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.5rem; }
  .issue-id { font-family: monospace; font-size: 0.75rem; color: var(--color-fg-muted); }
  .issue-title { margin: 0; font-size: 1rem; flex: 1; min-width: 200px; }
  .issue-group {
    font-size: 0.75rem;
    color: var(--color-accent);
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0.1rem 0.5rem;
  }
  .issue-files { display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.4rem; }
  .file-chip {
    font-family: monospace; font-size: 0.75rem;
    background: var(--color-bg-inset); border: 1px solid var(--color-border);
    border-radius: 4px; padding: 0.1rem 0.5rem; color: var(--color-accent);
  }
</style>
