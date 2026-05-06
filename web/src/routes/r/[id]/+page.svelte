<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { fetchResult, fetchResultsForRepo, subscribeToResult } from '$lib/api';
  import type { ResultDocument, ResultSummary } from '$lib/types';
  import { register, paletteItems, type PaletteItem } from '$lib/keyboard';
  import Mermaid from '$lib/components/Mermaid.svelte';
  import Mindmap from '$lib/components/Mindmap.svelte';
  import MarkdownView from '$lib/components/MarkdownView.svelte';
  import SeverityBadge from '$lib/components/SeverityBadge.svelte';
  import GroupSidebar from '$lib/components/GroupSidebar.svelte';
  import DiffViewer from '$lib/components/DiffViewer.svelte';
  import RunMetadataPanel from '$lib/components/RunMetadataPanel.svelte';
  import RepoHistoryNav from '$lib/components/RepoHistoryNav.svelte';
  import RerunButton from '$lib/components/RerunButton.svelte';
  import WhatView from '$lib/components/WhatView.svelte';
  import { statusColor } from '$lib/util/date';

  /** True when content has a mindmap/markmap fenced block. */
  function hasMindmap(content: string | undefined): boolean {
    return !!content && /```(?:mindmap|markmap)\n/.test(content);
  }

  /** True when the section is `ready` or `error` (i.e. not currently loading). */
  function canRerun(state: string | undefined): boolean {
    return state === 'ready' || state === 'error';
  }

  let doc: ResultDocument | null = null;
  let error = '';
  let loading = true;
  let selectedGroupId = 'g0';
  let unsubscribe: (() => void) | null = null;
  let sidebarCollapsed = false;
  let viewMode: 'review' | 'split' | 'diff' = 'review';
  let sidebarView: 'group' | 'file' = 'group';
  let selectedFile: string | null = null;
  // Initialize from `window` if available (client) or default to false (SSR-safe).
  // `false` is the conservative default — Split is the augmenting feature; absence
  // shouldn't be momentarily inflated to presence.
  let viewportWide = typeof window !== 'undefined' && window.innerWidth >= 1280;

  $: resultId = $page.params.id as string;
  $: selectedGroup = doc?.groups.find(g => g.id === selectedGroupId) ?? doc?.groups[0] ?? null;
  $: selectedReview = doc && selectedGroupId ? doc.reviews[selectedGroupId] : null;

  $: totalIssues = doc
    ? Object.values(doc.reviews).reduce((acc, r) => acc + (r.verdict_issues?.length ?? 0), 0)
    : 0;

  $: viewModeKey = doc ? `view-mode:${doc.id}` : '';

  // Auto-downgrade split → diff if viewport too narrow.
  $: if (!viewportWide && viewMode === 'split') {
    viewMode = 'diff';
  }

  function setViewMode(m: 'review' | 'split' | 'diff') {
    if (m === 'split' && !viewportWide) return;
    viewMode = m;
    if (viewModeKey) {
      try { localStorage.setItem(viewModeKey, m); } catch { /* ignore */ }
    }
  }

  async function loadResult() {
    try {
      doc = await fetchResult(resultId);
      loading = false;
      if (doc && doc.groups.length > 0) {
        selectedGroupId = doc.groups[0].id;
      }
      // Restore per-result view mode
      if (doc) {
        try {
          const saved = localStorage.getItem(`view-mode:${doc.id}`);
          if (saved === 'review' || saved === 'split' || saved === 'diff') {
            viewMode = saved;
          }
        } catch { /* ignore */ }
        try {
          const savedSide = localStorage.getItem(`sidebar-view:${doc.id}`);
          if (savedSide === 'group' || savedSide === 'file') {
            sidebarView = savedSide;
          }
        } catch { /* ignore */ }
      }
      // Always subscribe to SSE so rerun updates are received even after
      // the initial run completes.
      unsubscribe = subscribeToResult(
        resultId,
        async () => { doc = await fetchResult(resultId); },
        async () => { doc = await fetchResult(resultId); }
      );
    } catch (e) {
      error = String(e);
      loading = false;
    }
  }

  function cycleViewMode() {
    const order: Array<'review' | 'split' | 'diff'> = ['review', 'split', 'diff'];
    let i = order.indexOf(viewMode);
    for (let step = 0; step < 3; step++) {
      i = (i + 1) % order.length;
      const next = order[i];
      if (next === 'split' && !viewportWide) continue;
      setViewMode(next);
      return;
    }
  }

  function nextGroup(delta: 1 | -1) {
    if (!doc) return;
    const ids = doc.groups.map((g) => g.id);
    if (ids.length === 0) return;
    const idx = ids.indexOf(selectedGroupId);
    const next = ids[(idx + delta + ids.length) % ids.length];
    selectedGroupId = next;
    if (typeof window !== 'undefined') {
      queueMicrotask(() => {
        const el = document.querySelector('.main .group-header');
        if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      });
    }
  }

  function jumpToSection(name: 'WHY' | 'WHAT' | 'HOW' | 'VERDICT') {
    if (typeof document === 'undefined') return;
    // The section h3s contain the literal SECTION name. Scroll the matching one
    // (within `.main`) into view.
    const headings = document.querySelectorAll<HTMLElement>('.main .section-card h3');
    for (const h of Array.from(headings)) {
      if ((h.textContent || '').trim().toUpperCase() === name) {
        h.scrollIntoView({ behavior: 'smooth', block: 'start' });
        return;
      }
    }
  }

  function onResize() {
    viewportWide = typeof window !== 'undefined' && window.innerWidth >= 1280;
  }

  // Tablist keyboard navigation
  function onTabKey(e: KeyboardEvent) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const order: Array<'review' | 'split' | 'diff'> = ['review', 'split', 'diff'];
    const i = order.indexOf(viewMode);
    let nextIdx = e.key === 'ArrowRight' ? (i + 1) % 3 : (i + 2) % 3;
    let next = order[nextIdx];
    if (next === 'split' && !viewportWide) {
      // skip
      nextIdx = e.key === 'ArrowRight' ? (nextIdx + 1) % 3 : (nextIdx + 2) % 3;
      next = order[nextIdx];
    }
    setViewMode(next);
    e.preventDefault();
  }

  let repoHistory: ResultSummary[] = [];
  let unregs: Array<() => void> = [];
  let paletteHandle: { remove: () => void } | null = null;

  async function loadRepoHistory(name: string) {
    try {
      repoHistory = await fetchResultsForRepo(name);
    } catch {
      repoHistory = [];
    }
  }

  function navRepoHistory(delta: 1 | -1) {
    if (!doc || repoHistory.length < 2) return;
    const idx = repoHistory.findIndex((r) => r.id === doc!.id);
    if (idx < 0) return;
    const target = idx + delta;
    if (target < 0 || target >= repoHistory.length) return;
    void goto(`/r/${repoHistory[target].id}`);
  }

  // Refresh dynamic palette items whenever the document changes.
  function refreshPaletteItems() {
    if (paletteHandle) {
      paletteHandle.remove();
      paletteHandle = null;
    }
    if (!doc) return;
    const items: PaletteItem[] = [];
    for (const g of doc.groups) {
      const gid = g.id;
      items.push({
        id: `group:${gid}`,
        label: `Jump to group: ${g.label}`,
        group: 'Groups',
        action: () => { selectedGroupId = gid; },
      });
    }
    for (const f of doc.file_index ?? []) {
      const path = f.path;
      items.push({
        id: `file:${path}`,
        label: `Jump to file: ${path}`,
        group: 'Files',
        action: () => {
          selectedFile = path;
          const entry = doc?.file_index?.find((x) => x.path === path);
          const firstId = entry?.group_ids[0]
            ?? doc?.groups.find((g) => g.changes.some((c) => c.file === path))?.id;
          if (firstId) selectedGroupId = firstId;
        },
      });
    }
    for (const [gid, r] of Object.entries(doc.reviews)) {
      for (const issue of r.verdict_issues ?? []) {
        const issueId = issue.id;
        const targetGid = gid;
        items.push({
          id: `issue:${gid}:${issueId}`,
          label: `Jump to issue: ${issueId} ${issue.title}`,
          group: 'Issues',
          action: () => {
            selectedGroupId = targetGid;
            if (typeof window !== 'undefined') {
              queueMicrotask(() => {
                const el = document.getElementById(`issue-${issueId}`);
                el?.scrollIntoView({ behavior: 'smooth', block: 'start' });
              });
            }
          },
        });
      }
    }
    paletteItems.update((cur) => [...cur, ...items]);
    paletteHandle = {
      remove: () => {
        const ids = new Set(items.map((i) => i.id));
        paletteItems.update((cur) => cur.filter((i) => !ids.has(i.id)));
      },
    };
  }

  $: if (doc) refreshPaletteItems();
  $: if (doc?.repo?.name) void loadRepoHistory(doc.repo.name);

  onMount(() => {
    try {
      sidebarCollapsed = localStorage.getItem('sidebar-collapsed') === '1';
    } catch { /* ignore */ }
    onResize();
    window.addEventListener('resize', onResize);
    loadResult();

    // Register review-detail shortcuts.
    unregs.push(register({
      combo: 'j',
      scope: 'review-detail',
      label: 'Next group',
      group: 'Navigation',
      handler: () => nextGroup(1),
    }));
    unregs.push(register({
      combo: 'k',
      scope: 'review-detail',
      label: 'Previous group',
      group: 'Navigation',
      handler: () => nextGroup(-1),
    }));
    unregs.push(register({
      combo: '1',
      scope: 'review-detail',
      label: 'Jump to WHY',
      group: 'Sections',
      handler: () => jumpToSection('WHY'),
    }));
    unregs.push(register({
      combo: '2',
      scope: 'review-detail',
      label: 'Jump to WHAT',
      group: 'Sections',
      handler: () => jumpToSection('WHAT'),
    }));
    unregs.push(register({
      combo: '3',
      scope: 'review-detail',
      label: 'Jump to HOW',
      group: 'Sections',
      handler: () => jumpToSection('HOW'),
    }));
    unregs.push(register({
      combo: '4',
      scope: 'review-detail',
      label: 'Jump to VERDICT',
      group: 'Sections',
      handler: () => jumpToSection('VERDICT'),
    }));
    unregs.push(register({
      combo: 'v',
      scope: 'review-detail',
      label: 'Cycle view (review/split/diff)',
      group: 'View',
      handler: () => cycleViewMode(),
    }));
    unregs.push(register({
      combo: '[',
      scope: 'review-detail',
      label: 'Previous review in repo',
      group: 'Navigation',
      handler: () => navRepoHistory(1), // history is recency-sorted: prev review = older = idx+1
    }));
    unregs.push(register({
      combo: ']',
      scope: 'review-detail',
      label: 'Next review in repo',
      group: 'Navigation',
      handler: () => navRepoHistory(-1),
    }));
  });

  onDestroy(() => {
    unsubscribe?.();
    if (typeof window !== 'undefined') {
      window.removeEventListener('resize', onResize);
    }
    for (const u of unregs) u();
    unregs = [];
    if (paletteHandle) {
      paletteHandle.remove();
      paletteHandle = null;
    }
  });

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

  // Set of group ids that touch the currently-selected file (file view only).
  // In group view, returns the empty set so no dimming applies.
  $: highlightedGroupIds = (() => {
    if (sidebarView !== 'file' || !selectedFile || !doc) return new Set<string>();
    const entry = doc.file_index?.find(f => f.path === selectedFile);
    if (entry && entry.group_ids.length > 0) return new Set(entry.group_ids);
    // Fall back to scanning groups[].changes if file_index didn't supply group_ids.
    const matched = doc.groups
      .filter(g => g.changes.some(c => c.file === selectedFile))
      .map(g => g.id);
    return new Set(matched);
  })();

  function onSelectFile(e: CustomEvent<{ file: string }>) {
    selectedFile = e.detail.file;
    if (!doc) return;
    // Jump to the first group that touches this file so the main content updates.
    const entry = doc.file_index?.find(f => f.path === selectedFile);
    const firstId = entry?.group_ids[0]
      ?? doc.groups.find(g => g.changes.some(c => c.file === selectedFile))?.id;
    if (firstId && firstId !== selectedGroupId) {
      selectedGroupId = firstId;
    }
    if (typeof window !== 'undefined') {
      // Defer to next tick so the new group has rendered.
      queueMicrotask(() => {
        const el = document.querySelector('.main .group-header');
        if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
        // Also nudge the sidebar group card into view (no-op if already visible).
        if (firstId) {
          const sb = document.getElementById(`sb-group-${firstId}`);
          sb?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }
      });
    }
  }
</script>

{#if loading}
  <div class="page-loading">Loading review…</div>
{:else if error}
  <div class="page-error">Error: {error}</div>
{:else if doc}
  <div class="layout">
    <!-- Header -->
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
      <!-- Sidebar column: segmented switch + GroupSidebar -->
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
        <GroupSidebar
          groups={doc.groups}
          reviews={doc.reviews}
          bind:selectedGroupId
          collapsed={sidebarCollapsed}
          view={sidebarView}
          files={doc.file_index ?? []}
          {selectedFile}
          {highlightedGroupIds}
          on:selectFile={onSelectFile}
          on:toggleCollapsed={(e) => setSidebarCollapsed(e.detail)}
        />
      </div>

      <!-- Main content -->
      <main class="main">
        {#if selectedGroup && selectedReview}
          <div class="group-header">
            <h2>{selectedGroup.label}</h2>
            <p class="group-desc">{selectedGroup.description}</p>
            <div class="group-files">
              {#each selectedGroup.changes as change}
                <span class="file-chip">{change.file}</span>
              {/each}
            </div>
          </div>

          <!-- Tabs: Review | Issues -->
          <div class="tab-strip">
            <span class="tab tab-active" aria-current="page">Review</span>
            <a class="tab" href="/r/{doc.id}/issues">
              Issues{#if totalIssues > 0}<span class="tab-count">({totalIssues})</span>{/if}
            </a>
          </div>

          <!-- View-mode toggle -->
          <div class="view-toggle" role="tablist" aria-label="View mode">
            {#each ['review', 'split', 'diff'] as const as m}
              {@const disabled = m === 'split' && !viewportWide}
              <button
                type="button"
                role="tab"
                aria-selected={viewMode === m}
                aria-disabled={disabled}
                class="view-btn"
                class:active={viewMode === m}
                class:disabled
                tabindex={viewMode === m ? 0 : -1}
                on:click={() => !disabled && setViewMode(m)}
                on:keydown={onTabKey}
              >
                {m === 'review' ? 'Review' : m === 'split' ? 'Split' : 'Diff'}
              </button>
            {/each}
            <span class="view-hint" aria-hidden="true">press <kbd>v</kbd> to cycle</span>
          </div>

          {#if viewMode === 'diff'}
            <section class="section-card">
              {#if selectedGroup.unified_diff}
                <DiffViewer unifiedDiff={selectedGroup.unified_diff} />
              {:else}
                <p class="diff-fallback">Diff not available — re-run review to capture per-group diff.</p>
              {/if}
            </section>
          {:else if viewMode === 'split'}
            <div class="split">
              <div class="split-left">
                {#each ['WHY', 'WHAT', 'HOW', 'VERDICT'] as section}
                  <section class="section-card section-card--prose">
                    <div class="section-head">
                      <h3>{section}</h3>
                      {#if canRerun(selectedReview.sections[section]?.state)}
                        <RerunButton
                          resultId={doc.id}
                          groupId={selectedGroupId}
                          section={section.toLowerCase() as 'why' | 'what' | 'how' | 'verdict'}
                        />
                      {/if}
                    </div>
                    {#if selectedReview.sections[section]?.state === 'loading'}
                      <div class="skeleton">Loading…</div>
                    {:else if selectedReview.sections[section]?.state === 'ready'}
                      {#if section === 'HOW'}
                        <Mermaid content={selectedReview.sections[section].content ?? ''} />
                      {:else if section === 'WHY' && hasMindmap(selectedReview.sections[section]?.content)}
                        <Mindmap content={selectedReview.sections[section].content ?? ''} />
                      {:else if section === 'WHAT'}
                        <WhatView content={selectedReview.sections[section].content ?? ''} />
                      {:else if section === 'VERDICT'}
                        {#if selectedReview.verdict_issues && selectedReview.verdict_issues.length > 0}
                          <div class="issues">
                            {#each selectedReview.verdict_issues as issue}
                              <article class="issue" id="issue-{issue.id}">
                                <header class="issue-header">
                                  <SeverityBadge severity={issue.severity} />
                                  <span class="issue-id">{issue.id}</span>
                                  <h4>{issue.title}</h4>
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
                          <details class="raw-verdict">
                            <summary>Raw VERDICT</summary>
                            <MarkdownView content={selectedReview.sections[section].content ?? ''} />
                          </details>
                        {:else}
                          <MarkdownView content={selectedReview.sections[section].content ?? ''} />
                        {/if}
                      {:else}
                        <MarkdownView content={selectedReview.sections[section].content ?? ''} />
                      {/if}
                    {:else if selectedReview.sections[section]?.state === 'error'}
                      <div class="section-error">{selectedReview.sections[section].content}</div>
                    {/if}
                  </section>
                {/each}
              </div>
              <div class="split-right">
                <section class="section-card split-diff">
                  {#if selectedGroup.unified_diff}
                    <DiffViewer unifiedDiff={selectedGroup.unified_diff} />
                  {:else}
                    <p class="diff-fallback">Diff not available — re-run review to capture per-group diff.</p>
                  {/if}
                </section>
              </div>
            </div>
          {:else}
            <!-- review mode (default) -->
            <!-- WHY -->
            <section class="section-card section-card--prose">
              <div class="section-head">
                <h3>WHY</h3>
                {#if canRerun(selectedReview.sections.WHY?.state)}
                  <RerunButton resultId={doc.id} groupId={selectedGroupId} section="why" />
                {/if}
              </div>
              {#if selectedReview.sections.WHY?.state === 'loading'}
                <div class="skeleton">Analyzing intent…</div>
              {:else if selectedReview.sections.WHY?.state === 'ready'}
                {#if hasMindmap(selectedReview.sections.WHY.content)}
                  <Mindmap content={selectedReview.sections.WHY.content ?? ''} />
                {:else}
                  <MarkdownView content={selectedReview.sections.WHY.content ?? ''} />
                {/if}
              {:else if selectedReview.sections.WHY?.state === 'error'}
                <div class="section-error">{selectedReview.sections.WHY.content}</div>
              {/if}
            </section>

            <!-- WHAT -->
            <section class="section-card section-card--prose">
              <div class="section-head">
                <h3>WHAT</h3>
                {#if canRerun(selectedReview.sections.WHAT?.state)}
                  <RerunButton resultId={doc.id} groupId={selectedGroupId} section="what" />
                {/if}
              </div>
              {#if selectedReview.sections.WHAT?.state === 'loading'}
                <div class="skeleton">Analyzing changes…</div>
              {:else if selectedReview.sections.WHAT?.state === 'ready'}
                <WhatView content={selectedReview.sections.WHAT.content ?? ''} />
              {:else if selectedReview.sections.WHAT?.state === 'error'}
                <div class="section-error">{selectedReview.sections.WHAT.content}</div>
              {/if}
            </section>

            <!-- HOW -->
            <section class="section-card">
              <div class="section-head">
                <h3>HOW</h3>
                {#if canRerun(selectedReview.sections.HOW?.state)}
                  <RerunButton resultId={doc.id} groupId={selectedGroupId} section="how" />
                {/if}
              </div>
              {#if selectedReview.sections.HOW?.state === 'loading'}
                <div class="skeleton">Generating diagram…</div>
              {:else if selectedReview.sections.HOW?.state === 'ready'}
                <Mermaid content={selectedReview.sections.HOW.content ?? ''} />
              {:else if selectedReview.sections.HOW?.state === 'error'}
                <div class="section-error">{selectedReview.sections.HOW.content}</div>
              {/if}
            </section>

            <!-- VERDICT -->
            <section class="section-card section-card--prose">
              <div class="section-head">
                <h3>VERDICT</h3>
                {#if canRerun(selectedReview.sections.VERDICT?.state)}
                  <RerunButton resultId={doc.id} groupId={selectedGroupId} section="verdict" />
                {/if}
              </div>
              {#if selectedReview.sections.VERDICT?.state === 'loading'}
                <div class="skeleton">Reviewing for issues…</div>
              {:else if selectedReview.sections.VERDICT?.state === 'ready'}
                {#if selectedReview.verdict_issues && selectedReview.verdict_issues.length > 0}
                  <div class="issues">
                    {#each selectedReview.verdict_issues as issue}
                      <article class="issue" id="issue-{issue.id}">
                        <header class="issue-header">
                          <SeverityBadge severity={issue.severity} />
                          <span class="issue-id">{issue.id}</span>
                          <h4>{issue.title}</h4>
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
                  <details class="raw-verdict">
                    <summary>Raw VERDICT</summary>
                    <MarkdownView content={selectedReview.sections.VERDICT.content ?? ''} />
                  </details>
                {:else}
                  <MarkdownView content={selectedReview.sections.VERDICT.content ?? ''} />
                {/if}
              {:else if selectedReview.sections.VERDICT?.state === 'error'}
                <div class="section-error">{selectedReview.sections.VERDICT.content}</div>
              {/if}
            </section>
          {/if}

          {#if doc.metadata}
            <details class="run-details">
              <summary>Run details</summary>
              <RunMetadataPanel metadata={doc.metadata} repo={doc.repo} />
            </details>
          {/if}
        {:else}
          <div class="no-groups">No groups found.</div>
        {/if}
      </main>
    </div>
  </div>
{/if}

<style>
  .page-loading, .page-error, .no-groups {
    display: flex; align-items: center; justify-content: center;
    height: 50vh; color: var(--color-fg-muted);
  }
  .page-error { color: var(--color-danger); }
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

  .group-header { margin-bottom: 1rem; }
  .group-header h2 { margin: 0 0 0.25rem; font-size: 1.3rem; }
  .group-desc { color: var(--color-fg-muted); margin: 0 0 0.75rem; }
  .group-files { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .file-chip {
    font-family: monospace; font-size: 0.75rem;
    background: var(--color-bg-inset); border: 1px solid var(--color-border);
    border-radius: 4px; padding: 0.1rem 0.5rem; color: var(--color-accent);
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

  .view-toggle {
    display: inline-flex; align-items: center;
    gap: 0; margin-bottom: 1rem;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    overflow: hidden;
  }
  .view-btn {
    background: transparent;
    border: none;
    padding: 0.35rem 0.85rem;
    color: var(--color-fg-muted);
    font-size: 0.8rem;
    cursor: pointer;
    border-right: 1px solid var(--color-border);
  }
  .view-btn:last-of-type { border-right: none; }
  .view-btn:hover:not(.disabled) { background: var(--color-bg-elev); color: var(--color-fg); }
  .view-btn.active {
    background: var(--color-accent);
    color: var(--color-bg);
    font-weight: 600;
  }
  .view-btn.disabled { opacity: 0.4; cursor: not-allowed; }
  .view-hint {
    margin-left: 0.75rem;
    font-size: 0.7rem;
    color: var(--color-fg-muted);
    border: none;
  }
  .view-hint kbd {
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0 0.3rem;
    font-family: monospace;
    font-size: 0.7rem;
  }

  .section-card {
    background: var(--color-bg-elev); border: 1px solid var(--color-border); border-radius: 8px;
    padding: 1.25rem; margin-bottom: 1rem;
  }
  .section-card--prose :global(.markdown-body) {
    max-width: var(--reading-max);
  }
  .section-card h3 { margin: 0 0 0.75rem; font-size: 0.9rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-fg-muted); }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }
  .section-head h3 { margin: 0; }
  .skeleton {
    height: 80px; border-radius: 4px;
    background: linear-gradient(90deg, var(--color-bg-inset) 25%, var(--color-border) 50%, var(--color-bg-inset) 75%);
    background-size: 200% 100%; animation: shimmer 1.5s infinite;
    display: flex; align-items: center; padding: 1rem; color: var(--color-fg-muted);
  }
  @keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }
  .section-error { color: var(--color-danger); font-size: 0.85rem; }
  .diff-fallback { color: var(--color-fg-muted); font-style: italic; margin: 0; }

  .issues { display: flex; flex-direction: column; gap: 0.75rem; margin-bottom: 1rem; }
  .issue {
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 0.75rem 1rem;
    background: var(--color-bg);
  }
  .issue-header { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.4rem; }
  .issue-header h4 { margin: 0; font-size: 0.95rem; flex: 1; }
  .issue-id { font-family: monospace; font-size: 0.75rem; color: var(--color-fg-muted); }
  .issue-files { display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.4rem; }
  .raw-verdict { margin-top: 0.5rem; }
  .raw-verdict summary { cursor: pointer; color: var(--color-fg-muted); font-size: 0.8rem; }

  .split {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }
  .split-left, .split-right { min-width: 0; }
  .split-right .split-diff {
    position: sticky;
    top: calc(var(--app-nav-h) + var(--header-h) + 1rem);
    max-height: calc(100vh - var(--app-nav-h) - var(--header-h) - 2rem);
    overflow: auto;
  }

  .run-details {
    margin-top: 1rem;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-elev);
    padding: 0.75rem 1rem;
  }
  .run-details > summary {
    cursor: pointer;
    font-size: 0.85rem;
    color: var(--color-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .run-details[open] > summary { margin-bottom: 0.75rem; }

  @media (max-width: 768px) {
    .body, .body.sidebar-collapsed {
      grid-template-columns: 1fr;
    }
    .split { grid-template-columns: 1fr; }
  }
</style>
