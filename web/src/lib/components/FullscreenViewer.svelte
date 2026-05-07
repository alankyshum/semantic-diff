<script lang="ts">
  /**
   * Fullscreen overlay for diagrams (mermaid SVG + markmap mind maps).
   * Adapted from the `share--markdown` skill — same pan/zoom/search UX.
   *
   * Usage: bind `open`, set `kind` + `sourceEl` to the inline `.mermaid` /
   * `.mindmap-container` element, and pass an `onClose` callback. The viewer
   * clones the inline SVG into a full-viewport pannable canvas; for markmap
   * it re-creates a Markmap instance from the original transformer output
   * stashed on the source element.
   */
  import { tick, onMount, onDestroy } from 'svelte';

  export let open: boolean = false;
  export let kind: 'mermaid' | 'markmap' | null = null;
  export let sourceEl: HTMLElement | null = null;
  export let onClose: () => void = () => {};

  let viewportEl: HTMLDivElement | null = null;
  let mountedNode: SVGSVGElement | HTMLElement | null = null;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let mmInstance: any = null;

  // Pan/zoom state (mermaid only — markmap has its own).
  let scale = 1;
  let tx = 0;
  let ty = 0;
  let isDragging = false;
  let lastX = 0;
  let lastY = 0;

  // Mind-map search state.
  let searchQuery = '';
  let matchCount = 0;
  let matchIndex = 0;
  let matches: SVGGElement[] = [];

  // React when `open` flips.
  let lastOpen = false;
  $: if (open !== lastOpen) {
    lastOpen = open;
    if (open && kind && sourceEl) {
      tick().then(() => mountContent());
    } else {
      cleanup();
    }
  }

  // Re-run search when query changes (mind map only).
  $: if (open && kind === 'markmap' && searchQuery !== undefined) {
    queueMicrotask(runSearch);
  }

  async function mountContent() {
    if (!viewportEl || !sourceEl) return;
    if (kind === 'mermaid') {
      const svg = sourceEl.querySelector('svg');
      if (!svg) return;
      const clone = svg.cloneNode(true) as SVGSVGElement;
      clone.removeAttribute('width');
      clone.removeAttribute('height');
      clone.style.width = '100%';
      clone.style.height = '100%';
      clone.style.maxWidth = 'none';
      clone.style.maxHeight = 'none';
      viewportEl.replaceChildren(clone);
      mountedNode = clone;
      scale = 1;
      tx = 0;
      ty = 0;
      applyTransform();
    } else if (kind === 'markmap') {
      // The markmap host stashes the transformer-produced root on the element
      // when we mount it inline. Use the same root for the fullscreen view so
      // expand/collapse state is consistent.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const root = (sourceEl as any).__markmapRoot;
      if (!root) {
        // No stashed root — re-derive from `data-mindmap` if present.
        const md = decodeURIComponent(sourceEl.dataset?.mindmap || '');
        if (!md) return;
        const { Transformer } = await import('markmap-lib');
        const { Markmap } = await import('markmap-view');
        const { root: rerooted } = new Transformer().transform(md);
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.style.width = '100%';
        svg.style.height = '100%';
        viewportEl.replaceChildren(svg);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        mmInstance = Markmap.create(svg, { fitRatio: 0.95, duration: 200 } as any, rerooted);
        mountedNode = svg;
      } else {
        const { Markmap } = await import('markmap-view');
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.style.width = '100%';
        svg.style.height = '100%';
        viewportEl.replaceChildren(svg);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        mmInstance = Markmap.create(svg, { fitRatio: 0.95, duration: 200 } as any, root);
        mountedNode = svg;
      }
      searchQuery = '';
      matches = [];
      matchCount = 0;
      matchIndex = 0;
    }
  }

  function cleanup() {
    if (viewportEl) viewportEl.replaceChildren();
    mountedNode = null;
    mmInstance = null;
  }

  function applyTransform() {
    if (!mountedNode || kind !== 'mermaid') return;
    (mountedNode as SVGSVGElement).style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
    (mountedNode as SVGSVGElement).style.transformOrigin = '0 0';
  }

  function handleWheel(e: WheelEvent) {
    if (kind !== 'mermaid') return;
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    const next = Math.max(0.2, Math.min(8, scale * factor));
    if (viewportEl) {
      const r = viewportEl.getBoundingClientRect();
      const px = e.clientX - r.left;
      const py = e.clientY - r.top;
      tx = px - (px - tx) * (next / scale);
      ty = py - (py - ty) * (next / scale);
    }
    scale = next;
    applyTransform();
  }

  function handleMouseDown(e: MouseEvent) {
    if (kind !== 'mermaid') return;
    isDragging = true;
    lastX = e.clientX;
    lastY = e.clientY;
  }
  function handleMouseMove(e: MouseEvent) {
    if (!isDragging) return;
    tx += e.clientX - lastX;
    ty += e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;
    applyTransform();
  }
  function handleMouseUp() {
    isDragging = false;
  }

  function resetView() {
    if (kind === 'mermaid') {
      scale = 1; tx = 0; ty = 0;
      applyTransform();
    } else if (kind === 'markmap' && mmInstance) {
      mmInstance.fit();
    }
  }
  function zoomBy(factor: number) {
    if (kind === 'mermaid') {
      scale = Math.max(0.2, Math.min(8, scale * factor));
      applyTransform();
    } else if (kind === 'markmap' && mmInstance) {
      mmInstance.rescale(factor);
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') onClose();
    else if (e.key === '0') resetView();
    else if (e.key === '+' || e.key === '=') zoomBy(1.2);
    else if (e.key === '-') zoomBy(1 / 1.2);
  }

  function runSearch() {
    if (kind !== 'markmap' || !mountedNode) {
      matches = [];
      matchCount = 0;
      return;
    }
    const svg = mountedNode as SVGSVGElement;
    const all = svg.querySelectorAll<SVGGElement>('g.markmap-node');
    all.forEach((n) => n.classList.remove('search-match', 'search-current'));
    if (!searchQuery.trim()) {
      matches = [];
      matchCount = 0;
      matchIndex = 0;
      return;
    }
    const q = searchQuery.toLowerCase();
    const found: SVGGElement[] = [];
    all.forEach((n) => {
      const text = (n.textContent || '').toLowerCase();
      if (text.includes(q)) {
        n.classList.add('search-match');
        found.push(n);
      }
    });
    matches = found;
    matchCount = found.length;
    matchIndex = 0;
    focusMatch();
  }
  function focusMatch() {
    if (!matches.length || !mmInstance) return;
    matches.forEach((n) => n.classList.remove('search-current'));
    const node = matches[matchIndex];
    node.classList.add('search-current');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const datum = (node as any).__data__;
    if (datum && mmInstance.ensureView) {
      mmInstance.ensureView(datum, { left: 80, right: 80, top: 80, bottom: 80 });
    } else {
      node.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'center' });
    }
  }
  function nextMatch() {
    if (!matches.length) return;
    matchIndex = (matchIndex + 1) % matches.length;
    focusMatch();
  }
  function prevMatch() {
    if (!matches.length) return;
    matchIndex = (matchIndex - 1 + matches.length) % matches.length;
    focusMatch();
  }

  onMount(() => {
    if (typeof window !== 'undefined') {
      window.addEventListener('keydown', handleKey);
      window.addEventListener('mouseup', handleMouseUp);
      window.addEventListener('mousemove', handleMouseMove);
    }
  });
  onDestroy(() => {
    if (typeof window !== 'undefined') {
      window.removeEventListener('keydown', handleKey);
      window.removeEventListener('mouseup', handleMouseUp);
      window.removeEventListener('mousemove', handleMouseMove);
    }
  });
</script>

{#if open}
  <div
    class="overlay"
    role="dialog"
    aria-modal="true"
    aria-label="Fullscreen diagram viewer"
  >
    <div class="toolbar">
      <span class="kind-label">{kind === 'markmap' ? 'Mind map' : 'Mermaid'}</span>
      {#if kind === 'markmap'}
        <input
          class="search-input"
          type="search"
          placeholder="Search nodes…"
          bind:value={searchQuery}
        />
        {#if matchCount > 0}
          <span class="match-count">{matchIndex + 1}/{matchCount}</span>
          <button class="tb-btn" on:click={prevMatch} title="Previous match">↑</button>
          <button class="tb-btn" on:click={nextMatch} title="Next match">↓</button>
        {:else if searchQuery}
          <span class="match-count">no matches</span>
        {/if}
      {/if}
      <div class="tb-spacer"></div>
      <button class="tb-btn" on:click={() => zoomBy(1 / 1.2)} title="Zoom out (-)">−</button>
      <button class="tb-btn" on:click={resetView} title="Reset (0)">⊙</button>
      <button class="tb-btn" on:click={() => zoomBy(1.2)} title="Zoom in (+)">+</button>
      <button class="tb-btn close" on:click={onClose} title="Close (Esc)">✕</button>
    </div>
    <div
      bind:this={viewportEl}
      class="viewport"
      class:pan-cursor={kind === 'mermaid'}
      on:wheel={handleWheel}
      on:mousedown={handleMouseDown}
      role="presentation"
    ></div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--color-bg);
    z-index: 1000;
    display: flex;
    flex-direction: column;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-elev);
    flex-shrink: 0;
  }
  .kind-label {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-fg-muted);
    font-weight: 600;
  }
  .search-input {
    flex: 0 0 280px;
    padding: 0.3rem 0.6rem;
    border: 1px solid var(--color-border);
    border-radius: 5px;
    background: var(--color-bg);
    color: var(--color-fg);
    font-size: 0.85rem;
  }
  .search-input:focus {
    outline: 2px solid var(--color-accent);
    outline-offset: -1px;
    border-color: transparent;
  }
  .match-count {
    font-size: 0.75rem;
    color: var(--color-fg-muted);
    font-variant-numeric: tabular-nums;
  }
  .tb-spacer { flex: 1; }
  .tb-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 28px;
    padding: 0 0.4rem;
    background: transparent;
    color: var(--color-fg-muted);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    cursor: pointer;
    font-size: 0.9rem;
  }
  .tb-btn:hover {
    background: var(--color-bg-inset);
    color: var(--color-fg);
  }
  .tb-btn.close { border-color: transparent; }
  .tb-btn.close:hover {
    background: rgba(220, 50, 50, 0.15);
    color: var(--color-danger);
  }
  .viewport {
    flex: 1;
    overflow: hidden;
    position: relative;
    background: var(--color-bg);
  }
  .viewport.pan-cursor { cursor: grab; }
  .viewport.pan-cursor:active { cursor: grabbing; }

  /* Mind-map search highlights — applied via classList on g.markmap-node. */
  :global(g.markmap-node.search-match > line),
  :global(g.markmap-node.search-match > circle) {
    stroke: var(--color-warning, #f5a623) !important;
    stroke-width: 3 !important;
  }
  :global(g.markmap-node.search-current > line),
  :global(g.markmap-node.search-current > circle) {
    stroke: var(--color-accent, #d97706) !important;
    stroke-width: 4 !important;
  }
  :global(g.markmap-node.search-match foreignObject) {
    background: rgba(245, 166, 35, 0.15);
  }
</style>
