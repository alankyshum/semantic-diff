<script lang="ts">
  import { effectiveTheme } from '$lib/stores/theme';
  import MarkdownView from './MarkdownView.svelte';

  export let content: string;

  type ContentPart = { kind: 'mindmap'; body: string } | { kind: 'prose'; text: string };

  let svgHtml = '';
  let parts: ContentPart[] = [];
  let error = '';

  /** Split content into interleaved prose and mindmap segments. */
  function splitContent(raw: string): ContentPart[] {
    const re = /```(?:mindmap|markmap)\n([\s\S]*?)```/g;
    const out: ContentPart[] = [];
    let lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(raw)) !== null) {
      const prose = raw.slice(lastIndex, m.index).trim();
      if (prose) out.push({ kind: 'prose', text: prose });
      out.push({ kind: 'mindmap', body: m[1].trim() });
      lastIndex = m.index + m[0].length;
    }
    const trailing = raw.slice(lastIndex).trim();
    if (trailing) out.push({ kind: 'prose', text: trailing });
    return out;
  }

  /** Check if the content has any mindmap/markmap fenced blocks. */
  export function hasMindmap(raw: string): boolean {
    return /```(?:mindmap|markmap)\n/.test(raw);
  }

  async function renderMindmap(markdown: string, theme: 'light' | 'dark') {
    try {
      const { Transformer } = await import('markmap-lib');
      const { Markmap } = await import('markmap-view');

      const transformer = new Transformer();
      const { root } = transformer.transform(markdown);

      // Create a temporary container for the SVG
      const container = document.createElement('div');
      container.style.width = '100%';
      container.style.height = '400px';
      container.style.position = 'absolute';
      container.style.left = '-9999px';
      document.body.appendChild(container);

      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('style', 'width: 100%; height: 400px;');
      container.appendChild(svg);

      const mm = Markmap.create(svg, {
        colorFreezeLevel: 2,
        initialExpandLevel: 3,
        duration: 0,
      });
      mm.setData(root);
      await mm.fit();

      svgHtml = svg.outerHTML;

      // Clean up
      document.body.removeChild(container);
      error = '';
    } catch (e) {
      error = `Mindmap render error: ${e}`;
      svgHtml = '';
    }
  }

  let lastRenderSig = '';
  $: {
    const sig = `${$effectiveTheme}|${content ?? ''}`;
    if (content && sig !== lastRenderSig) {
      lastRenderSig = sig;
      parts = splitContent(content);
      // Find the first mindmap block to render
      const mmPart = parts.find((p): p is { kind: 'mindmap'; body: string } => p.kind === 'mindmap');
      if (mmPart) {
        renderMindmap(mmPart.body, $effectiveTheme);
      }
    }
  }
</script>

{#each parts as part}
  {#if part.kind === 'mindmap'}
    {#if error}
      <div class="mindmap-error">{error}</div>
    {:else if svgHtml}
      <div class="mindmap-container" class:dark={$effectiveTheme === 'dark'}>
        {@html svgHtml}
      </div>
    {:else}
      <div class="mindmap-loading">Rendering mindmap…</div>
    {/if}
  {:else if part.kind === 'prose'}
    <div class="mindmap-prose">
      <MarkdownView content={part.text} />
    </div>
  {/if}
{/each}

<style>
  .mindmap-container {
    overflow-x: auto;
    background: var(--color-bg);
    border-radius: 6px;
    padding: 0.5rem;
    margin: 0.5rem 0 0.75rem 0;
    min-height: 200px;
  }
  .mindmap-container :global(svg) { width: 100%; height: 400px; }
  .mindmap-container.dark :global(text) { fill: var(--color-fg); }
  .mindmap-error { color: var(--color-danger); font-size: 0.85rem; }
  .mindmap-loading { color: var(--color-fg-muted); font-style: italic; }
  .mindmap-prose { max-width: var(--reading-max, 80ch); margin-bottom: 0.75rem; }
</style>
