<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  export let unifiedDiff: string = '';
  export let fileFilter: string[] | null = null; // if set, only show these files

  let container: HTMLDivElement;
  let html = '';
  let loaded = false;

  // Filter unified diff to only include hunks for the given files
  function filterDiff(raw: string, files: string[]): string {
    if (!files || files.length === 0) return raw;

    const lines = raw.split('\n');
    const result: string[] = [];
    let inTarget = false;

    for (const line of lines) {
      if (line.startsWith('diff --git ')) {
        // Check if this file is in our filter
        inTarget = files.some((f) => line.includes(f));
      }
      if (inTarget) {
        result.push(line);
      }
    }

    return result.join('\n');
  }

  async function render() {
    if (!container || !unifiedDiff) return;

    const { html: diff2htmlHtml } = await import('diff2html');

    const diff = fileFilter ? filterDiff(unifiedDiff, fileFilter) : unifiedDiff;

    if (!diff.trim()) {
      html = '<p class="no-diff">No diff content for this group.</p>';
      return;
    }

    html = diff2htmlHtml(diff, {
      drawFileList: fileFilter === null,
      matching: 'lines',
      outputFormat: 'side-by-side',
      renderNothingWhenEmpty: false,
    });
  }

  onMount(async () => {
    // Lazy load diff2html CSS
    if (!document.getElementById('diff2html-css')) {
      const link = document.createElement('link');
      link.id = 'diff2html-css';
      link.rel = 'stylesheet';
      link.href = 'https://cdnjs.cloudflare.com/ajax/libs/diff2html/3.4.48/diff2html.min.css';
      document.head.appendChild(link);
    }
    loaded = true;
    await render();
  });

  $: if (loaded && (unifiedDiff || fileFilter)) {
    render();
  }
</script>

<div class="diff-viewer" bind:this={container}>
  {#if !unifiedDiff}
    <p class="empty-state">No diff available.</p>
  {:else if !html}
    <p class="loading-state">Loading diff...</p>
  {:else}
    {@html html}
  {/if}
</div>

<style>
  .diff-viewer {
    overflow-x: auto;
    font-family: 'SFMono-Regular', 'Consolas', 'Liberation Mono', 'Menlo', monospace;
    font-size: 0.8125rem;
    line-height: 1.45;
  }

  .empty-state,
  .loading-state,
  .no-diff {
    padding: 1rem;
    color: var(--color-fg-muted);
    font-style: italic;
    font-family: inherit;
  }

  /* Override diff2html styles to use theme tokens */
  :global(.d2h-wrapper) {
    border-radius: 0.375rem;
    overflow: hidden;
    border: 1px solid var(--color-border);
  }

  :global(.d2h-file-header) {
    background-color: var(--color-bg-elev);
    border-bottom: 1px solid var(--color-border);
    padding: 0.5rem 1rem;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--color-fg);
  }

  :global(.d2h-del) {
    background-color: var(--color-diff-del-bg);
  }

  :global(.d2h-ins) {
    background-color: var(--color-diff-add-bg);
  }

  :global(.d2h-del-changes) {
    background-color: var(--color-diff-del-emph);
  }

  :global(.d2h-ins-changes) {
    background-color: var(--color-diff-add-emph);
  }

  :global(.d2h-code-linenumber) {
    color: var(--color-fg-muted);
    user-select: none;
    min-width: 3rem;
  }
</style>
