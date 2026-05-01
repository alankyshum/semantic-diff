<script lang="ts">
  export let content: string;

  import { marked } from 'marked';

  // Configure marked for GitHub-flavored markdown
  marked.setOptions({ gfm: true, breaks: false });

  $: html = content ? marked(content) as string : '';
</script>

<div class="markdown-body">
  {@html html}
</div>

<style>
  .markdown-body :global(table) {
    border-collapse: collapse; width: 100%; font-size: 0.875rem;
  }
  .markdown-body :global(th), .markdown-body :global(td) {
    padding: 0.4rem 0.75rem; border: 1px solid #30363d; text-align: left;
  }
  .markdown-body :global(th) { background: #21262d; font-weight: 600; }
  .markdown-body :global(tr:nth-child(even)) { background: #161b22; }
  .markdown-body :global(ul), .markdown-body :global(ol) { padding-left: 1.5rem; }
  .markdown-body :global(li) { margin-bottom: 0.25rem; }
  .markdown-body :global(code) {
    background: #21262d; border-radius: 3px; padding: 0.1em 0.3em;
    font-size: 0.875em; color: #f85149;
  }
  .markdown-body :global(pre code) { background: none; padding: 0; color: inherit; }
  .markdown-body :global(pre) {
    background: #21262d; border-radius: 6px; padding: 1rem; overflow-x: auto;
  }
  .markdown-body :global(h2) { margin-top: 1.5rem; margin-bottom: 0.5rem; font-size: 1rem; }
  .markdown-body :global(h3) { margin-top: 1rem; font-size: 0.95rem; }

  /* Risk color coding in WHAT tables */
  .markdown-body :global(td:last-child) { font-weight: 600; }
  :global(.risk-high) { color: #f85149; }
  :global(.risk-medium) { color: #d29922; }
  :global(.risk-low) { color: #3fb950; }
</style>
