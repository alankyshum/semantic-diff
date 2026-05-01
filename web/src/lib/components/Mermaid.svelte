<script lang="ts">
  import { onMount, afterUpdate } from 'svelte';

  export let content: string;

  let container: HTMLDivElement;
  let rendered = '';
  let error = '';

  async function renderMermaid() {
    if (!content) return;

    // Extract mermaid block from content
    const match = content.match(/```mermaid\n([\s\S]*?)```/);
    const diagramText = match ? match[1].trim() : content.trim();

    try {
      const mermaid = (await import('mermaid')).default;
      mermaid.initialize({ startOnLoad: false, theme: 'dark', securityLevel: 'loose' });

      const id = `mermaid-${Math.random().toString(36).slice(2)}`;
      const { svg } = await mermaid.render(id, diagramText);
      rendered = svg;
      error = '';
    } catch (e) {
      error = `Mermaid render error: ${e}`;
      rendered = '';
    }
  }

  onMount(renderMermaid);
  $: content && renderMermaid();
</script>

{#if error}
  <div class="mermaid-error">
    <p>{error}</p>
    <pre class="raw-content">{content}</pre>
  </div>
{:else if rendered}
  <div class="mermaid-container" bind:this={container}>
    {@html rendered}
  </div>
{:else}
  <div class="mermaid-loading">Rendering diagram…</div>
{/if}

<style>
  .mermaid-container { overflow-x: auto; background: #0d1117; border-radius: 6px; padding: 1rem; }
  .mermaid-container :global(svg) { max-width: 100%; height: auto; }
  .mermaid-error { color: #f85149; }
  .mermaid-loading { color: #8b949e; font-style: italic; }
  .raw-content { font-size: 0.75rem; color: #8b949e; white-space: pre-wrap; word-break: break-all; }
</style>
