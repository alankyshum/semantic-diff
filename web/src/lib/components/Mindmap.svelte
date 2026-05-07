<script lang="ts" context="module">
  /** Module-level helper so callers (e.g. +page.svelte) can decide whether
   *  to use the Mindmap component. Matches the `share--markdown` skill which
   *  treats `mindmap`, `markmap`, and mermaid `mindmap` blocks identically. */
  export function hasMindmap(raw: string | undefined): boolean {
    if (!raw) return false;
    if (/```(?:mindmap|markmap)\n/.test(raw)) return true;
    // Mermaid `mindmap` fenced blocks
    return /```mermaid\n\s*(?:%%[^\n]*\n\s*)*mindmap\b/.test(raw);
  }
</script>

<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { effectiveTheme } from '$lib/stores/theme';
  import MarkdownView from './MarkdownView.svelte';
  import { mermaidMindmapToMarkdown, isMermaidMindmap } from '$lib/util/mindmap-mermaid';

  export let content: string;

  type ContentPart =
    | { kind: 'mindmap'; body: string; idx: number }
    | { kind: 'prose'; text: string };

  let parts: ContentPart[] = [];
  let containerEl: HTMLElement;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mmInstances: any[] = [];

  /** Split content into interleaved prose and mindmap segments.
   *  Mindmap blocks come from `mindmap`/`markmap` fences OR mermaid mindmaps. */
  function splitContent(raw: string): ContentPart[] {
    const out: ContentPart[] = [];
    // Match either ```mindmap|markmap``` OR ```mermaid``` (we'll filter the
    // latter to mindmap-only later)
    const re = /```(mindmap|markmap|mermaid)\n([\s\S]*?)```/g;
    let lastIndex = 0;
    let mmIdx = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(raw)) !== null) {
      const lang = m[1].toLowerCase();
      const body = m[2];
      // For mermaid blocks, only treat as mindmap when the inner content
      // begins with the `mindmap` keyword.
      if (lang === 'mermaid' && !isMermaidMindmap(body)) {
        // Leave as-is (the Mermaid component will render this); we render
        // it as a passthrough fenced block in prose.
        // Continue accumulating prose; do nothing.
        continue;
      }
      const prose = raw.slice(lastIndex, m.index).trim();
      if (prose) out.push({ kind: 'prose', text: prose });
      const md = lang === 'mermaid'
        ? (mermaidMindmapToMarkdown(body) ?? '')
        : body.trim();
      out.push({ kind: 'mindmap', body: md, idx: mmIdx++ });
      lastIndex = m.index + m[0].length;
    }
    const trailing = raw.slice(lastIndex).trim();
    if (trailing) out.push({ kind: 'prose', text: trailing });
    return out;
  }

  function destroyInstances() {
    for (const mm of mmInstances) {
      try { mm?.destroy?.(); } catch { /* ignore */ }
    }
    mmInstances = [];
  }

  async function renderMindmaps() {
    if (!containerEl) return;
    destroyInstances();
    const { Transformer } = await import('markmap-lib');
    const { Markmap } = await import('markmap-view');
    const transformer = new Transformer();

    const hosts = containerEl.querySelectorAll<HTMLElement>('[data-mindmap-host]');
    hosts.forEach((host) => {
      const md = decodeURIComponent(host.dataset.mindmap || '');
      if (!md) return;
      // Reset host content
      host.innerHTML = '';
      try {
        const { root } = transformer.transform(md);
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.setAttribute('style', 'width: 100%; height: 380px;');
        host.appendChild(svg);
        // `colorFreezeLevel` is a `deriveOptions` field rather than a direct
        // IMarkmapOptions field — pass through `as any` since the lib accepts
        // it at runtime and the share--markdown skill uses the same trick.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const mm = Markmap.create(svg, {
          colorFreezeLevel: 2,
          initialExpandLevel: 3,
          duration: 250,
        } as any);
        mm.setData(root);
        // Defer fit so SVG has a measured size.
        queueMicrotask(() => { void mm.fit(); });
        mmInstances.push(mm);
      } catch (e) {
        host.innerHTML = `<div class="mindmap-error">Mindmap render error: ${e}</div>`;
      }
    });
  }

  let lastRenderSig = '';
  $: {
    const sig = `${$effectiveTheme}|${content ?? ''}`;
    if (content && sig !== lastRenderSig) {
      lastRenderSig = sig;
      parts = splitContent(content);
      tick().then(() => { void renderMindmaps(); });
    }
  }

  onDestroy(() => {
    destroyInstances();
  });
</script>

<div bind:this={containerEl} class="mindmap-root" class:dark={$effectiveTheme === 'dark'}>
  {#each parts as part, i (i)}
    {#if part.kind === 'mindmap'}
      <div
        class="mindmap-container"
        data-mindmap-host
        data-mindmap={encodeURIComponent(part.body)}
      >
        <div class="mindmap-loading">Rendering mindmap…</div>
      </div>
    {:else if part.kind === 'prose'}
      <div class="mindmap-prose">
        <MarkdownView content={part.text} />
      </div>
    {/if}
  {/each}
</div>

<style>
  .mindmap-root { display: contents; }
  .mindmap-container {
    overflow: hidden;
    background: var(--color-bg);
    border-radius: 6px;
    padding: 0.5rem;
    margin: 0.5rem 0 0.75rem 0;
    min-height: 200px;
    position: relative;
  }
  .mindmap-container :global(svg) { width: 100%; height: 380px; }
  .mindmap-root.dark :global(.markmap-foreign),
  .mindmap-root.dark .mindmap-container :global(text) { fill: var(--color-fg); }
  /* Error message rendered inside a host via innerHTML — needs :global. */
  .mindmap-container :global(.mindmap-error) { color: var(--color-danger); font-size: 0.85rem; }
  .mindmap-loading {
    color: var(--color-fg-muted);
    font-style: italic;
    padding: 1rem;
  }
  .mindmap-prose { max-width: var(--reading-max, 80ch); margin-bottom: 0.75rem; }
</style>
