<script lang="ts">
  import { effectiveTheme } from '$lib/stores/theme';
  import MarkdownView from './MarkdownView.svelte';
  import { renderWithAutoFix } from '$lib/util/mermaid-lint';
  import {
    isMermaidPie, isMermaidXyChart,
    parseMermaidPie, parseMermaidXyChart, parseChartExtension,
    renderPieChart, renderXyChart, renderEnhancedXyChart,
    renderJsonChart, applyChartTheme,
  } from '$lib/util/charts';
  import { tick } from 'svelte';

  export let content: string;

  type Block =
    | { kind: 'svg'; svg: string; caption: string | null; id: string; error: string; warnings: string[] }
    | { kind: 'chart'; chartKind: 'pie' | 'xy' | 'xy-ext' | 'json'; payload: unknown; caption: string | null; id: string; error: string }
    | { kind: 'error'; caption: string | null; id: string; error: string };
  type ContentPart =
    | { kind: 'mermaid'; index: number }
    | { kind: 'chart-json'; jsonText: string; index: number }
    | { kind: 'prose'; text: string };

  let blocks: Block[] = [];
  let parts: ContentPart[] = [];

  /** Split raw content into interleaved prose, mermaid and chart segments. */
  function splitContent(raw: string): {
    mermaidBlocks: Array<{ body: string; caption: string | null }>;
    chartJsonBlocks: string[];
    parts: ContentPart[];
  } {
    // Combined regex matches either ```mermaid…``` or ```chart…```
    const re = /```(mermaid|chart)\n([\s\S]*?)```/g;
    const mermaidBlocks: Array<{ body: string; caption: string | null }> = [];
    const chartJsonBlocks: string[] = [];
    const contentParts: ContentPart[] = [];
    let lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(raw)) !== null) {
      const prose = raw.slice(lastIndex, m.index).trim();
      if (prose) contentParts.push({ kind: 'prose', text: prose });
      const lang = m[1].toLowerCase();
      if (lang === 'chart') {
        chartJsonBlocks.push(m[2]);
        contentParts.push({ kind: 'chart-json', jsonText: m[2], index: chartJsonBlocks.length - 1 });
      } else {
        mermaidBlocks.push(parseBlock(m[2]));
        contentParts.push({ kind: 'mermaid', index: mermaidBlocks.length - 1 });
      }
      lastIndex = m.index + m[0].length;
    }
    const trailing = raw.slice(lastIndex).trim();
    if (trailing) contentParts.push({ kind: 'prose', text: trailing });
    // Legacy fallback: no fenced blocks at all AND the content actually looks
    // like a mermaid diagram (first non-empty line starts with a known
    // diagram keyword) → treat entire content as one mermaid block.
    //
    // Without this guard, an LLM-generated HOW section that violates its
    // prompt and emits only prose (or prose+```rust) gets handed to mermaid
    // wholesale, which then fails with cryptic YAML errors on the prose. We'd
    // rather render that as plain markdown.
    if (mermaidBlocks.length === 0 && chartJsonBlocks.length === 0) {
      if (looksLikeMermaid(raw)) {
        mermaidBlocks.push(parseBlock(raw));
        return {
          mermaidBlocks,
          chartJsonBlocks,
          parts: [{ kind: 'mermaid', index: 0 }],
        };
      }
      // Otherwise: render the whole thing as markdown prose.
      return {
        mermaidBlocks,
        chartJsonBlocks,
        parts: raw.trim() ? [{ kind: 'prose', text: raw }] : [],
      };
    }
    return { mermaidBlocks, chartJsonBlocks, parts: contentParts };
  }

  /** Recognised mermaid diagram-type keywords (the ones the HOW prompt
   *  whitelists, plus a couple of widely-used aliases). Used by the legacy
   *  fallback to distinguish "naked" mermaid source from arbitrary prose. */
  const MERMAID_KEYWORDS = [
    'flowchart', 'graph', 'sequenceDiagram', 'classDiagram',
    'stateDiagram', 'stateDiagram-v2', 'erDiagram',
    'journey', 'gantt', 'pie', 'mindmap', 'timeline',
    'xychart-beta', 'gitGraph', 'requirementDiagram',
  ];

  function looksLikeMermaid(raw: string): boolean {
    // Strip leading `%%` comment lines and find the first content line.
    const lines = raw.split(/\r?\n/);
    for (const line of lines) {
      const t = line.trim();
      if (!t) continue;
      if (t.startsWith('%%')) continue;
      // First real line — does it start with a known keyword?
      return MERMAID_KEYWORDS.some(kw => t.startsWith(kw));
    }
    return false;
  }

  function parseBlock(body: string): { body: string; caption: string | null } {
    const trimmed = body.replace(/^\n+/, '').replace(/\s+$/, '');
    const firstLine = trimmed.split('\n', 1)[0] ?? '';
    const capMatch = firstLine.match(/^\s*%%\s*(.+?)\s*$/);
    const caption = capMatch ? capMatch[1] : null;
    return { body: trimmed, caption };
  }

  /** Build a placeholder Block for a mermaid pie/xychart so the renderer
   *  knows to mount Chart.js after DOM ready. Returns null if the source
   *  isn't a chart-type mermaid block. */
  function tryBuildChartBlock(body: string, caption: string | null, idx: number): Block | null {
    const id = `chart-${Math.random().toString(36).slice(2)}-${idx}`;
    if (isMermaidPie(body)) {
      const parsed = parseMermaidPie(body);
      if (parsed) return { kind: 'chart', chartKind: 'pie', payload: parsed, caption, id, error: '' };
    }
    if (isMermaidXyChart(body)) {
      const parsed = parseMermaidXyChart(body);
      if (!parsed) return null;
      const ext = parseChartExtension(body);
      return ext
        ? { kind: 'chart', chartKind: 'xy-ext', payload: { parsed, ext }, caption, id, error: '' }
        : { kind: 'chart', chartKind: 'xy', payload: parsed, caption, id, error: '' };
    }
    return null;
  }

  async function renderAll(theme: 'light' | 'dark' = 'dark') {
    if (!content) return;
    applyChartTheme(theme === 'dark');
    const { mermaidBlocks, chartJsonBlocks, parts: contentParts } = splitContent(content);
    parts = contentParts;

    // Pre-pass: detect which mermaid blocks are pie/xychart → handle via Chart.js.
    const blockMap: Block[] = mermaidBlocks.map((p, i) => {
      const charty = tryBuildChartBlock(p.body, p.caption, i);
      if (charty) return charty;
      return { kind: 'svg', svg: '', caption: p.caption, id: `mermaid-${i}`, error: '', warnings: [] };
    });

    const mermaidNeeded = blockMap.some(b => b.kind === 'svg');
    let mermaid: typeof import('mermaid').default | null = null;
    if (mermaidNeeded) {
      try {
        mermaid = (await import('mermaid')).default;
        mermaid.initialize({
          startOnLoad: false,
          theme: theme === 'dark' ? 'dark' : 'default',
          securityLevel: 'loose',
        });
      } catch (e) {
        for (let i = 0; i < blockMap.length; i++) {
          if (blockMap[i].kind === 'svg') {
            blockMap[i] = {
              kind: 'error',
              caption: blockMap[i].caption,
              id: blockMap[i].id,
              error: `Mermaid load error: ${e}`,
            };
          }
        }
      }
    }

    if (mermaid) {
      await Promise.all(
        mermaidBlocks.map(async (p, i) => {
          if (blockMap[i].kind !== 'svg') return; // chart already handled
          const id = `mermaid-${Math.random().toString(36).slice(2)}-${i}`;
          try {
            const { svg, warnings } = await renderWithAutoFix(mermaid!, p.body, id);
            blockMap[i] = { kind: 'svg', svg, caption: p.caption, id, error: '', warnings };
          } catch (e) {
            blockMap[i] = {
              kind: 'error',
              caption: p.caption,
              id,
              error: `Mermaid render error: ${e}`,
            };
          }
        }),
      );
    }
    blocks = blockMap;
    // After Svelte commits, mount the chart canvases.
    await tick();
    mountCharts(theme === 'dark');
    mountJsonCharts(theme === 'dark');
    // Track json blocks for length so we can skip mounted ones.
    void chartJsonBlocks;
  }

  /** For each `chart` block placeholder in the DOM, build a Chart.js canvas
   *  and replace the placeholder. Idempotent (skips already-mounted hosts). */
  function mountCharts(dark: boolean) {
    if (typeof document === 'undefined') return;
    document.querySelectorAll<HTMLElement>('[data-chart-host]').forEach(host => {
      if (host.dataset.chartMounted === '1') return;
      const id = host.dataset.chartId!;
      const block = blocks.find(b => b.id === id);
      if (!block || block.kind !== 'chart') return;
      let el: HTMLElement;
      try {
        switch (block.chartKind) {
          case 'pie':
            el = renderPieChart(block.payload as Parameters<typeof renderPieChart>[0], { dark });
            break;
          case 'xy':
            el = renderXyChart(block.payload as Parameters<typeof renderXyChart>[0], { dark });
            break;
          case 'xy-ext': {
            const { parsed, ext } = block.payload as {
              parsed: Parameters<typeof renderEnhancedXyChart>[0];
              ext: Parameters<typeof renderEnhancedXyChart>[1];
            };
            el = renderEnhancedXyChart(parsed, ext, { dark });
            break;
          }
          default:
            return;
        }
      } catch (e) {
        host.textContent = `Chart render error: ${e}`;
        host.dataset.chartMounted = '1';
        return;
      }
      host.replaceChildren(el);
      host.dataset.chartMounted = '1';
    });
  }

  function mountJsonCharts(dark: boolean) {
    if (typeof document === 'undefined') return;
    document.querySelectorAll<HTMLElement>('[data-chart-json-host]').forEach(host => {
      if (host.dataset.chartMounted === '1') return;
      const json = decodeURIComponent(host.dataset.chartJson || '');
      const { element, error } = renderJsonChart(json, dark);
      if (error) {
        host.textContent = error;
      } else {
        host.replaceChildren(element);
      }
      host.dataset.chartMounted = '1';
    });
  }

  // Re-render when content or theme changes.
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
  {#each parts as part, partIdx (partIdx)}
    {#if part.kind === 'mermaid'}
      {@const block = blocks[part.index]}
      {#if block}
        <figure class="mermaid-container">
          {#if block.caption}
            <figcaption class="mermaid-caption">{block.caption}</figcaption>
          {/if}
          {#if block.kind === 'error'}
            <div class="mermaid-error">
              <p>{block.error}</p>
              <pre class="raw-content">{content}</pre>
            </div>
          {:else if block.kind === 'svg'}
            {#if block.warnings.length > 0}
              <div class="mermaid-warnings">
                {#each block.warnings as w}<span class="mermaid-warn-tag">{w}</span>{/each}
              </div>
            {/if}
            {@html block.svg}
            <button
              type="button"
              class="diagram-expand-btn"
              title="Open fullscreen"
              aria-label="Open diagram fullscreen"
              on:click|stopPropagation={(e) => {
                const fig = (e.currentTarget as HTMLElement).closest('figure');
                if (!fig) return;
                fig.dispatchEvent(new CustomEvent('diagram-expand', {
                  bubbles: true,
                  detail: { kind: 'mermaid', el: fig },
                }));
              }}
            >⛶</button>
          {:else if block.kind === 'chart'}
            <div class="chart-placeholder" data-chart-host data-chart-id={block.id}>
              <div class="chart-placeholder-inner">Rendering chart…</div>
            </div>
          {/if}
        </figure>
      {/if}
    {:else if part.kind === 'chart-json'}
      <figure class="mermaid-container">
        <div
          class="chart-placeholder"
          data-chart-json-host
          data-chart-json={encodeURIComponent(part.jsonText)}
        >
          <div class="chart-placeholder-inner">Rendering chart…</div>
        </div>
      </figure>
    {:else if part.kind === 'prose'}
      <div class="mermaid-prose">
        <MarkdownView content={part.text} />
      </div>
    {/if}
  {/each}
{/if}

<style>
  .mermaid-container {
    overflow-x: auto;
    background: var(--color-bg);
    border-radius: 6px;
    padding: 1rem;
    margin: 0 0 0.75rem 0;
    position: relative;
  }
  .mermaid-container :global(svg) { max-width: 100%; height: auto; }

  /* Fullscreen expand button overlay (mermaid + markmap share the class via :global). */
  .diagram-expand-btn {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    z-index: 2;
    background: var(--color-bg-elev);
    border: 1px solid var(--color-border);
    color: var(--color-fg-muted);
    border-radius: 4px;
    width: 28px;
    height: 28px;
    cursor: pointer;
    font-size: 0.95rem;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0.7;
    transition: opacity 0.15s, color 0.15s, border-color 0.15s;
  }
  .mermaid-container:hover .diagram-expand-btn { opacity: 1; }
  .diagram-expand-btn:hover {
    color: var(--color-fg);
    border-color: var(--color-accent);
  }
  .mermaid-caption {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    color: var(--color-fg-muted);
    margin-bottom: 0.4rem;
  }
  .mermaid-error { color: var(--color-danger); }
  .mermaid-loading { color: var(--color-fg-muted); font-style: italic; }
  .mermaid-warnings { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 6px; }
  .mermaid-warn-tag {
    font-size: 0.7rem; padding: 1px 6px; border-radius: 3px;
    background: var(--color-warning, #d29922); color: var(--color-bg, #000);
    opacity: 0.8;
  }
  .raw-content { font-size: 0.75rem; color: var(--color-fg-muted); white-space: pre-wrap; word-break: break-all; }
  .mermaid-prose { max-width: var(--reading-max, 80ch); margin-bottom: 0.75rem; }

  /* Chart.js host */
  .chart-placeholder { width: 100%; min-height: 320px; }
  .chart-placeholder-inner {
    color: var(--color-fg-muted);
    font-size: 0.85rem;
    font-style: italic;
    padding: 1rem;
  }
  .mermaid-container :global(.chart-host) {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    width: 100%;
  }
  .mermaid-container :global(.chart-title) {
    font-size: 0.85rem;
    color: var(--color-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .mermaid-container :global(.chart-canvas-wrap) {
    position: relative;
    height: 320px;
    width: 100%;
  }

  /* :::changed highlighting — Mermaid translates :::changed to class="changed" on nodes */
  .mermaid-container :global(.changed > rect),
  .mermaid-container :global(.changed > polygon),
  .mermaid-container :global(.changed > circle),
  .mermaid-container :global(.changed > ellipse),
  .mermaid-container :global(.changed > path) {
    stroke: var(--color-warning, #d29922) !important;
    stroke-width: 2.5px !important;
    filter: drop-shadow(0 0 4px var(--color-warning, #d29922));
  }
  .mermaid-container :global(.changed > .nodeLabel) {
    font-weight: 700 !important;
  }
  .mermaid-container :global(.flowchart-link[style*="stroke:#d29922"]),
  .mermaid-container :global(.flowchart-link[style*="stroke: #d29922"]) {
    filter: drop-shadow(0 0 3px var(--color-warning, #d29922));
  }
</style>
