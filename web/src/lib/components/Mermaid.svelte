<script lang="ts">
  import { effectiveTheme } from '$lib/stores/theme';

  export let content: string;

  type Block = { svg: string; caption: string | null; id: string; error: string };

  let blocks: Block[] = [];

  function extractBlocks(raw: string): Array<{ body: string; caption: string | null }> {
    const re = /```mermaid\n([\s\S]*?)```/g;
    const out: Array<{ body: string; caption: string | null }> = [];
    let m: RegExpExecArray | null;
    while ((m = re.exec(raw)) !== null) {
      out.push(parseBlock(m[1]));
    }
    if (out.length === 0) {
      out.push(parseBlock(raw));
    }
    return out;
  }

  function parseBlock(body: string): { body: string; caption: string | null } {
    const trimmed = body.replace(/^\n+/, '').replace(/\s+$/, '');
    const firstLine = trimmed.split('\n', 1)[0] ?? '';
    const capMatch = firstLine.match(/^\s*%%\s*(.+?)\s*$/);
    const caption = capMatch ? capMatch[1] : null;
    return { body: trimmed, caption };
  }

  async function renderAll(theme: 'light' | 'dark' = 'dark') {
    if (!content) return;
    const parts = extractBlocks(content);

    let mermaid: typeof import('mermaid').default;
    try {
      mermaid = (await import('mermaid')).default;
      mermaid.initialize({
        startOnLoad: false,
        theme: theme === 'dark' ? 'dark' : 'default',
        securityLevel: 'loose',
      });
    } catch (e) {
      blocks = parts.map((p, i) => ({
        svg: '',
        caption: p.caption,
        id: `mermaid-${i}`,
        error: `Mermaid load error: ${e}`,
      }));
      return;
    }

    const rendered = await Promise.all(
      parts.map(async (p, i) => {
        const id = `mermaid-${Math.random().toString(36).slice(2)}-${i}`;
        try {
          const { svg } = await mermaid.render(id, p.body);
          return { svg, caption: p.caption, id, error: '' } as Block;
        } catch (e) {
          return {
            svg: '',
            caption: p.caption,
            id,
            error: `Mermaid render error: ${e}`,
          } as Block;
        }
      })
    );
    blocks = rendered;
  }

  // Re-render when content or theme changes.
  // Skip re-render when the (content, theme) signature is unchanged.
  let lastRenderSig = '';
  $: {
    const sig = `${$effectiveTheme}|${content ?? ''}`;
    if (content && sig !== lastRenderSig) {
      lastRenderSig = sig;
      renderAll($effectiveTheme);
    }
  }
</script>

{#if blocks.length === 0}
  <div class="mermaid-loading">Rendering diagram…</div>
{:else}
  {#each blocks as block (block.id)}
    <figure class="mermaid-container">
      {#if block.caption}
        <figcaption class="mermaid-caption">{block.caption}</figcaption>
      {/if}
      {#if block.error}
        <div class="mermaid-error">
          <p>{block.error}</p>
          <pre class="raw-content">{content}</pre>
        </div>
      {:else}
        {@html block.svg}
      {/if}
    </figure>
  {/each}
{/if}

<style>
  .mermaid-container {
    overflow-x: auto;
    background: var(--color-bg);
    border-radius: 6px;
    padding: 1rem;
    margin: 0 0 0.75rem 0;
  }
  .mermaid-container :global(svg) { max-width: 100%; height: auto; }
  .mermaid-caption {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    color: var(--color-fg-muted);
    margin-bottom: 0.4rem;
  }
  .mermaid-error { color: var(--color-danger); }
  .mermaid-loading { color: var(--color-fg-muted); font-style: italic; }
  .raw-content { font-size: 0.75rem; color: var(--color-fg-muted); white-space: pre-wrap; word-break: break-all; }
</style>
