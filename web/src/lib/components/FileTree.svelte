<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { FileEntry, Severity } from '$lib/types';
  import SeverityBadge from './SeverityBadge.svelte';

  // Component is self-recursive — it can be called with either:
  //   (a) a flat `files` array (top-level entry point), or
  //   (b) a pre-built `node` (recursive call from itself).
  export let files: FileEntry[] | null = null;
  export let selectedFile: string | null = null;

  // Internal recursive props (set when this instance is rendered as <svelte:self>).
  export let node: TreeNode | null = null;
  export let expanded: Set<string> | null = null;
  export let onToggle: ((path: string) => void) | null = null;
  export let onSelect: ((path: string) => void) | null = null;

  const dispatch = createEventDispatcher<{ select: string }>();

  // Severity rank — lower = higher priority. Keys are lowercase. We
  // defensively lowercase the input severity at lookup time so PascalCase
  // values from the Rust backend (e.g. "Critical") work alongside the
  // canonical lowercase form. See SeverityBadge.svelte for the same pattern.
  const SEV_RANK: Record<string, number> = {
    critical: 0, high: 1, medium: 2, low: 3, nit: 4, info: 5,
  };

  interface TreeNode {
    name: string;
    fullPath: string;
    isFile: boolean;
    fileEntry?: FileEntry;
    children: TreeNode[];
  }

  function buildTree(entries: FileEntry[]): TreeNode {
    interface RawNode {
      name: string;
      isFile: boolean;
      fileEntry?: FileEntry;
      children: Map<string, RawNode>;
    }
    const root: RawNode = { name: '', isFile: false, children: new Map() };

    for (const f of entries) {
      const parts = f.path.split('/').filter(p => p.length > 0);
      let cur = root;
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        const isLast = i === parts.length - 1;
        let next = cur.children.get(part);
        if (!next) {
          next = { name: part, isFile: false, children: new Map() };
          cur.children.set(part, next);
        }
        if (isLast) {
          next.isFile = true;
          next.fileEntry = f;
        }
        cur = next;
      }
    }

    function transform(n: RawNode, parentPath: string): TreeNode {
      let name = n.name;
      let cur = n;
      let fullPath = parentPath ? `${parentPath}/${name}` : name;

      // Collapse single-child directory chains (VS Code style).
      while (!cur.isFile && cur.children.size === 1) {
        const onlyChild = cur.children.values().next().value as RawNode;
        if (onlyChild.isFile) break;
        name = `${name}/${onlyChild.name}`;
        fullPath = parentPath ? `${parentPath}/${name}` : name;
        cur = onlyChild;
      }

      const childArr: TreeNode[] = [];
      for (const child of cur.children.values()) {
        childArr.push(transform(child, fullPath));
      }
      childArr.sort((a, b) => {
        if (a.isFile !== b.isFile) return a.isFile ? 1 : -1;
        return a.name.localeCompare(b.name);
      });

      return {
        name,
        fullPath,
        isFile: cur.isFile,
        fileEntry: cur.fileEntry,
        children: childArr,
      };
    }

    const out: TreeNode = { name: '', fullPath: '', isFile: false, children: [] };
    for (const child of root.children.values()) {
      out.children.push(transform(child, ''));
    }
    out.children.sort((a, b) => {
      if (a.isFile !== b.isFile) return a.isFile ? 1 : -1;
      return a.name.localeCompare(b.name);
    });
    return out;
  }

  function rollupSeverity(n: TreeNode): Severity | null {
    let best: Severity | null = null;
    let bestRank = Infinity;
    function visit(x: TreeNode) {
      if (x.isFile && x.fileEntry?.max_severity) {
        const sev = x.fileEntry.max_severity;
        const r = SEV_RANK[String(sev).toLowerCase()] ?? Infinity;
        if (r < bestRank) {
          bestRank = r;
          best = sev;
        }
      }
      for (const c of x.children) visit(c);
    }
    visit(n);
    return best;
  }

  // ---- Top-level mode (when `files` is provided) ----
  let topTree: TreeNode | null = null;
  let topExpanded = new Set<string>();
  let initialized = false;

  $: if (files !== null) {
    topTree = buildTree(files);
  }

  $: if (!initialized && topTree && topTree.children.length > 0) {
    const next = new Set<string>();
    for (const c of topTree.children) {
      if (!c.isFile) next.add(c.fullPath);
    }
    topExpanded = next;
    initialized = true;
  }

  function topToggle(path: string) {
    if (topExpanded.has(path)) topExpanded.delete(path);
    else topExpanded.add(path);
    topExpanded = new Set(topExpanded);
  }

  function topSelect(path: string) {
    selectedFile = path;
    dispatch('select', path);
  }

  // ---- Resolved props (works in both modes) ----
  $: resolvedNode = node;
  $: resolvedExpanded = expanded ?? topExpanded;
  $: resolvedToggle = onToggle ?? topToggle;
  $: resolvedSelect = onSelect ?? topSelect;
</script>

{#if files !== null}
  <!-- Top-level entry point: render root children. -->
  {#if !topTree || topTree.children.length === 0}
    <div class="empty">No files.</div>
  {:else}
    <ul class="tree root">
      {#each topTree.children as child (child.fullPath)}
        <svelte:self
          node={child}
          expanded={topExpanded}
          onToggle={topToggle}
          onSelect={topSelect}
          {selectedFile}
        />
      {/each}
    </ul>
  {/if}
{:else if resolvedNode}
  {@const n = resolvedNode}
  {#if n.isFile && n.fileEntry}
    <li>
      <button
        type="button"
        class="row file"
        class:selected={selectedFile === n.fileEntry.path}
        on:click={() => n.fileEntry && resolvedSelect(n.fileEntry.path)}
        title={n.fileEntry.path}
        data-path={n.fileEntry.path}
      >
        <span class="name">{n.name}</span>
        <span class="lines">
          {#if n.fileEntry.add_lines > 0}<span class="add">+{n.fileEntry.add_lines}</span>{/if}
          {#if n.fileEntry.del_lines > 0}<span class="del">-{n.fileEntry.del_lines}</span>{/if}
        </span>
        {#if n.fileEntry.max_severity}
          <SeverityBadge severity={n.fileEntry.max_severity} />
        {/if}
      </button>
    </li>
  {:else}
    {@const open = resolvedExpanded.has(n.fullPath)}
    {@const rollup = rollupSeverity(n)}
    <li>
      <button
        type="button"
        class="row dir"
        on:click={() => resolvedToggle(n.fullPath)}
        aria-expanded={open}
        title={n.fullPath}
        data-dir={n.fullPath}
      >
        <span class="chevron" aria-hidden="true">{open ? '▾' : '▸'}</span>
        <span class="folder" aria-hidden="true">📁</span>
        <span class="name">{n.name}</span>
        {#if rollup}
          <SeverityBadge severity={rollup} />
        {/if}
      </button>
      {#if open}
        <ul class="tree">
          {#each n.children as child (child.fullPath)}
            <svelte:self
              node={child}
              expanded={resolvedExpanded}
              onToggle={resolvedToggle}
              onSelect={resolvedSelect}
              {selectedFile}
            />
          {/each}
        </ul>
      {/if}
    </li>
  {/if}
{/if}

<style>
  .empty {
    padding: 0.75rem;
    color: var(--color-fg-muted);
    font-size: 0.85rem;
    text-align: center;
  }
  ul.tree {
    list-style: none;
    margin: 0;
    padding: 0;
    line-height: 1.5;
  }
  ul.root {
    padding: 0 0.25rem;
  }
  /* Nested <ul> inside a directory <li>: indent. <svelte:self> creates separate
     style scopes per recursion level, so target the nested ul via its parent li. */
  li > ul {
    padding-left: 1rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    color: var(--color-fg);
    padding: 0.15rem 0.4rem;
    font-size: 0.8rem;
    cursor: pointer;
    border-radius: 3px;
    min-width: 0;
  }
  .row:hover { background: var(--color-bg-elev); }
  .row.file.selected {
    background: var(--color-bg-inset);
    border-left: 2px solid var(--color-accent);
    padding-left: calc(0.4rem - 2px);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .chevron {
    width: 0.9rem;
    color: var(--color-fg-muted);
    font-size: 0.7rem;
  }
  .folder { font-size: 0.8rem; }
  .lines {
    display: inline-flex;
    gap: 0.3rem;
    font-family: monospace;
    font-size: 0.7rem;
  }
  .add { color: var(--color-success); }
  .del { color: var(--color-danger); }
</style>
