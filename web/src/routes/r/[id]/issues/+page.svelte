<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { fetchResult, subscribeToResult } from '$lib/api';
  import type { ResultDocument, Issue, Severity } from '$lib/types';
  import SeverityBadge from '$lib/components/SeverityBadge.svelte';
  import MarkdownView from '$lib/components/MarkdownView.svelte';
  import GroupSidebar from '$lib/components/GroupSidebar.svelte';
  import RepoHistoryNav from '$lib/components/RepoHistoryNav.svelte';
  import ReviewSkeleton from '$lib/components/ReviewSkeleton.svelte';
  import { statusColor } from '$lib/util/date';

  const ALL_SEVS: Severity[] = ['critical', 'high', 'medium', 'low', 'nit', 'info'];
  const SEV_RANK: Record<Severity, number> = {
    critical: 0, high: 1, medium: 2, low: 3, nit: 4, info: 5,
  };

  let doc: ResultDocument | null = null;
  let loading = true;
  let error = '';
  let unsubscribe: (() => void) | null = null;

  let sevFilter = new Set<Severity>(ALL_SEVS);
  let fileFilter = '';
  let groupFilter = '';

  // Sidebar state mirrors the review page so the layout looks identical.
  let sidebarCollapsed = false;
  let sidebarView: 'group' | 'file' = 'group';
  // Issues page tracks selectedGroupId only for sidebar selection. Selecting a
  // group narrows the list (mirrors clicking on the group filter dropdown).
  let selectedGroupId = '';
  let selectedFile: string | null = null;

  $: resultId = $page.params.id as string;

  // Read filters from URL on mount.
  function readUrl() {
    const sp = $page.url.searchParams;
    const sev = sp.get('severity');
    if (sev) {
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
      sidebarCollapsed = localStorage.getItem('sidebar-collapsed') === '1';
    } catch { /* ignore */ }
    try {
      doc = await fetchResult(resultId);
      try {
        const savedSide = localStorage.getItem(`sidebar-view:${doc.id}`);
        if (savedSide === 'group' || savedSide === 'file') sidebarView = savedSide;
      } catch { /* ignore */ }
      // Subscribe to SSE so this page also reflects live progress (groups
      // appearing, sections completing) — keeps the issue count fresh.
      unsubscribe = subscribeToResult(
        resultId,
        async () => { doc = await fetchResult(resultId); },
        async () => { doc = await fetchResult(resultId); },
      );
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  onDestroy(() => { unsubscribe?.(); });

  function setSidebarCollapsed(v: boolean) {
    sidebarCollapsed = v;
    try { localStorage.setItem('sidebar-collapsed', v ? '1' : '0'); } catch { /* ignore */ }
  }

  function setSidebarView(v: 'group' | 'file') {
    sidebarView = v;
    if (doc) {
      try { localStorage.setItem(`sidebar-view:${doc.id}`, v); } catch { /* ignore */ }
    }
  }

  function onSelectFile(e: CustomEvent<{ file: string }>) {
    selectedFile = e.detail.file;
    fileFilter = e.detail.file;
    writeUrl();
  }

  // Bridge sidebar click ↔ #group-filter dropdown.
  // We split this into two effects keyed on a single `lastSynced` cache so
  // Svelte's reactivity doesn't re-detect a cycle.
  let lastSynced = '';
  $: {
    // Whichever value most recently differs from `lastSynced` wins; we then
    // mirror it to the other binding and update `lastSynced`.
    if (selectedGroupId !== lastSynced && selectedGroupId !== groupFilter) {
      lastSynced = selectedGroupId;
      groupFilter = selectedGroupId;
      writeUrl();
    } else if (groupFilter !== lastSynced && groupFilter !== selectedGroupId) {
      lastSynced = groupFilter;
      selectedGroupId = groupFilter;
    }
  }

  interface FlatIssue extends Issue { groupId: string; groupLabel: string }

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

  $: totalIssues = allIssues.length;

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

  // Set of group ids highlighted by the currently-selected file (file view only).
  $: highlightedGroupIds = (() => {
    if (sidebarView !== 'file' || !selectedFile || !doc) return new Set<string>();
    const entry = doc.file_index?.find(f => f.path === selectedFile);
    if (entry && entry.group_ids.length > 0) return new Set(entry.group_ids);
    const matched = doc.groups
      .filter(g => g.changes.some(c => c.file === selectedFile))
      .map(g => g.id);
    return new Set(matched);
  })();
</script>

{#if loading}
  <ReviewSkeleton variant="page" rows={5} />
{:else if error}
  <div class="page-error">Error: {error}</div>
{:else if doc}
  <div class="layout">
    <header class="header">
      <div class="header-left">
        <a href="/" class="back">← All reviews</a>
        <h1>{doc.title}</h1>
        {#if doc.repo?.name}
          <a class="repo-chip" href="/?repo={encodeURIComponent(doc.repo.name)}" title="Filter by repo">
            {doc.repo.name}{#if doc.repo.branch}<span class="repo-branch">@{doc.repo.branch}</span>{/if}
          </a>
          <RepoHistoryNav repoName={doc.repo.name} currentId={doc.id} />
        {/if}
        <span class="status-badge" style="color: {statusColor(doc.status)}">{doc.status}</span>
      </div>
      <div class="header-meta">
        <span>{doc.diff.files.length} files</span>
        <span>{doc.groups.length} groups</span>
        <span class="doc-id">{doc.id}</span>
      </div>
    </header>

    <div class="body" class:sidebar-collapsed={sidebarCollapsed}>
      <div class="sidebar-col">
        {#if !sidebarCollapsed}
          <div class="sidebar-view-switch" role="tablist" aria-label="Sidebar view">
            <button
              type="button"
              role="tab"
              class="sv-btn"
              class:active={sidebarView === 'group'}
              aria-selected={sidebarView === 'group'}
              on:click={() => setSidebarView('group')}
            >By group</button>
            <button
              type="button"
              role="tab"
              class="sv-btn"
              class:active={sidebarView === 'file'}
              aria-selected={sidebarView === 'file'}
              on:click={() => setSidebarView('file')}
            >By file</button>
          </div>
        {/if}
        {#if doc.groups.length === 0 && doc.status === 'running'}
          <ReviewSkeleton variant="sidebar-only" rows={5} />
        {:else}
          <GroupSidebar
            groups={doc.groups}
            reviews={doc.reviews}
            bind:selectedGroupId
            on:selectFile={onSelectFile}
            on:toggleCollapsed={(e) => setSidebarCollapsed(e.detail)}
            collapsed={sidebarCollapsed}
            view={sidebarView}
            files={doc.file_index ?? []}
            {selectedFile}
            {highlightedGroupIds}
          />
        {/if}
      </div>

      <main class="main">
        <!-- Same tab strip the review page uses, so the two pages feel like
             two views of the same screen instead of two different routes. -->
        <div class="tab-strip">
          <a class="tab" href="/r/{doc.id}">Review</a>
          <span class="tab tab-active" aria-current="page">
            Issues{#if totalIssues > 0}<span class="tab-count">({totalIssues})</span>{/if}
          </span>
        </div>

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

        {#if doc.groups.length === 0 && doc.status === 'running'}
          <ReviewSkeleton variant="main-only" rows={3} />
        {:else if filtered.length === 0}
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
      </main>
    </div>
  </div>
{/if}

<style>
  .page-error { color: var(--color-danger); padding: 1rem; }

  /* Shell — kept identical to /r/[id]/+page.svelte so the two pages are
     visually indistinguishable apart from main content. */
  .layout { display: flex; flex-direction: column; min-height: 100vh; }
  .header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.75rem 1.5rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-elev);
    position: sticky; top: var(--app-nav-h); z-index: 10;
    height: var(--header-h);
  }
  .header-left { display: flex; align-items: center; gap: 0.75rem; }
  .back { color: var(--color-fg-muted); font-size: 0.85rem; }
  h1 { margin: 0; font-size: 1.1rem; }
  .repo-chip {
    font-size: 0.75rem;
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 999px;
    padding: 0.1rem 0.55rem;
    color: var(--color-accent);
  }
  .repo-chip:hover { border-color: var(--color-accent); text-decoration: none; }
  .repo-branch { color: var(--color-fg-muted); margin-left: 0.15rem; }
  .status-badge { font-size: 0.75rem; font-weight: 600; text-transform: uppercase; }
  .header-meta { display: flex; gap: 1rem; align-items: center; font-size: 0.8rem; color: var(--color-fg-muted); }
  .doc-id { font-family: monospace; }

  .body {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    flex: 1;
    min-height: 0;
  }
  .body.sidebar-collapsed {
    grid-template-columns: var(--sidebar-w-collapsed) 1fr;
  }
  .sidebar-col {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .sidebar-view-switch {
    display: flex;
    margin: 0.5rem 0.5rem 0;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    overflow: hidden;
    flex-shrink: 0;
  }
  .sv-btn {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--color-fg-muted);
    font-size: 0.75rem;
    padding: 0.3rem 0.4rem;
    cursor: pointer;
    border-right: 1px solid var(--color-border);
  }
  .sv-btn:last-child { border-right: none; }
  .sv-btn:hover { color: var(--color-fg); background: var(--color-bg-elev); }
  .sv-btn.active {
    background: var(--color-bg-inset);
    color: var(--color-fg);
    font-weight: 600;
  }
  .main {
    flex: 1;
    padding: 1.5rem;
    overflow-y: auto;
    width: 100%;
    max-width: var(--content-max);
    margin: 0 auto;
  }

  .tab-strip {
    display: flex; gap: 0.25rem;
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 0.75rem;
  }
  .tab {
    padding: 0.4rem 0.85rem;
    font-size: 0.85rem;
    color: var(--color-fg-muted);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tab:hover { color: var(--color-fg); text-decoration: none; }
  .tab-active {
    color: var(--color-fg);
    border-bottom-color: var(--color-accent);
    font-weight: 600;
  }
  .tab-count { color: var(--color-fg-muted); margin-left: 0.25rem; font-weight: 400; }

  /* Filters + issue list */
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

  .empty {
    color: var(--color-fg-muted);
    text-align: center;
    padding: 3rem;
    border: 1px dashed var(--color-border);
    border-radius: 8px;
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

  @media (max-width: 768px) {
    .body, .body.sidebar-collapsed {
      grid-template-columns: 1fr;
    }
  }
</style>
