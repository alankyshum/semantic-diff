<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { fetchResult, subscribeToResult } from '$lib/api';
  import type { ResultDocument, Group } from '$lib/types';
  import Mermaid from '$lib/components/Mermaid.svelte';
  import MarkdownView from '$lib/components/MarkdownView.svelte';
  import SeverityBadge from '$lib/components/SeverityBadge.svelte';
  import GroupSidebar from '$lib/components/GroupSidebar.svelte';

  let doc: ResultDocument | null = null;
  let error = '';
  let loading = true;
  let selectedGroupId = 'g0';
  let unsubscribe: (() => void) | null = null;

  $: resultId = $page.params.id;
  $: selectedGroup = doc?.groups.find(g => g.id === selectedGroupId) ?? doc?.groups[0] ?? null;
  $: selectedReview = doc && selectedGroupId ? doc.reviews[selectedGroupId] : null;

  async function loadResult() {
    try {
      doc = await fetchResult(resultId);
      loading = false;
      if (doc && doc.groups.length > 0) {
        selectedGroupId = doc.groups[0].id;
      }
      // Subscribe to updates if still running
      if (doc?.status === 'running') {
        unsubscribe = subscribeToResult(
          resultId,
          async () => { doc = await fetchResult(resultId); },
          async () => { doc = await fetchResult(resultId); }
        );
      }
    } catch (e) {
      error = String(e);
      loading = false;
    }
  }

  onMount(loadResult);
  onDestroy(() => unsubscribe?.());

  function statusColor(status: string): string {
    switch (status) {
      case 'complete': return '#3fb950';
      case 'running': return '#d29922';
      case 'failed': return '#f85149';
      default: return '#8b949e';
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
        <span class="status-badge" style="color: {statusColor(doc.status)}">{doc.status}</span>
      </div>
      <div class="header-meta">
        <span>{doc.diff.files.length} files</span>
        <span>{doc.groups.length} groups</span>
        <span class="doc-id">{doc.id}</span>
      </div>
    </header>

    <div class="body">
      <!-- Sidebar -->
      <GroupSidebar
        groups={doc.groups}
        reviews={doc.reviews}
        bind:selectedGroupId
      />

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

          <!-- WHY -->
          <section class="section-card">
            <h3>WHY</h3>
            {#if selectedReview.sections.WHY?.state === 'loading'}
              <div class="skeleton">Analyzing intent…</div>
            {:else if selectedReview.sections.WHY?.state === 'ready'}
              <MarkdownView content={selectedReview.sections.WHY.content ?? ''} />
            {:else if selectedReview.sections.WHY?.state === 'error'}
              <div class="section-error">{selectedReview.sections.WHY.content}</div>
            {/if}
          </section>

          <!-- WHAT -->
          <section class="section-card">
            <h3>WHAT</h3>
            {#if selectedReview.sections.WHAT?.state === 'loading'}
              <div class="skeleton">Analyzing changes…</div>
            {:else if selectedReview.sections.WHAT?.state === 'ready'}
              <MarkdownView content={selectedReview.sections.WHAT.content ?? ''} />
            {:else if selectedReview.sections.WHAT?.state === 'error'}
              <div class="section-error">{selectedReview.sections.WHAT.content}</div>
            {/if}
          </section>

          <!-- HOW -->
          <section class="section-card">
            <h3>HOW</h3>
            {#if selectedReview.sections.HOW?.state === 'loading'}
              <div class="skeleton">Generating diagram…</div>
            {:else if selectedReview.sections.HOW?.state === 'ready'}
              <Mermaid content={selectedReview.sections.HOW.content ?? ''} />
            {:else if selectedReview.sections.HOW?.state === 'error'}
              <div class="section-error">{selectedReview.sections.HOW.content}</div>
            {/if}
          </section>

          <!-- VERDICT -->
          <section class="section-card">
            <h3>VERDICT</h3>
            {#if selectedReview.sections.VERDICT?.state === 'loading'}
              <div class="skeleton">Reviewing for issues…</div>
            {:else if selectedReview.sections.VERDICT?.state === 'ready'}
              <MarkdownView content={selectedReview.sections.VERDICT.content ?? ''} />
            {:else if selectedReview.sections.VERDICT?.state === 'error'}
              <div class="section-error">{selectedReview.sections.VERDICT.content}</div>
            {/if}
          </section>
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
    height: 50vh; color: #8b949e;
  }
  .page-error { color: #f85149; }
  .layout { display: flex; flex-direction: column; min-height: 100vh; }
  .header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.75rem 1.5rem;
    border-bottom: 1px solid #30363d;
    background: #161b22;
    position: sticky; top: 0; z-index: 10;
  }
  .header-left { display: flex; align-items: center; gap: 0.75rem; }
  .back { color: #8b949e; font-size: 0.85rem; }
  h1 { margin: 0; font-size: 1.1rem; }
  .status-badge { font-size: 0.75rem; font-weight: 600; text-transform: uppercase; }
  .header-meta { display: flex; gap: 1rem; font-size: 0.8rem; color: #8b949e; }
  .doc-id { font-family: monospace; }

  .body { display: flex; flex: 1; }
  .main { flex: 1; padding: 1.5rem; overflow-y: auto; max-width: 900px; }

  .group-header { margin-bottom: 1.5rem; }
  .group-header h2 { margin: 0 0 0.25rem; font-size: 1.3rem; }
  .group-desc { color: #8b949e; margin: 0 0 0.75rem; }
  .group-files { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .file-chip {
    font-family: monospace; font-size: 0.75rem;
    background: #21262d; border: 1px solid #30363d;
    border-radius: 4px; padding: 0.1rem 0.5rem; color: #58a6ff;
  }

  .section-card {
    background: #161b22; border: 1px solid #30363d; border-radius: 8px;
    padding: 1.25rem; margin-bottom: 1rem;
  }
  .section-card h3 { margin: 0 0 0.75rem; font-size: 0.9rem; text-transform: uppercase; letter-spacing: 0.05em; color: #8b949e; }
  .skeleton {
    height: 80px; border-radius: 4px;
    background: linear-gradient(90deg, #21262d 25%, #30363d 50%, #21262d 75%);
    background-size: 200% 100%; animation: shimmer 1.5s infinite;
    display: flex; align-items: center; padding: 1rem; color: #8b949e;
  }
  @keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }
  .section-error { color: #f85149; font-size: 0.85rem; }
</style>
