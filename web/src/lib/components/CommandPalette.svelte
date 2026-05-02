<script lang="ts">
  // F17 — Command palette. Modal dialog launched by cmd+k. Lists registered
  // shortcuts plus dynamic palette items contributed by pages via
  // `paletteItems` store. Fuzzy-substring filter, arrow-key nav, Enter to run.
  import { onMount, onDestroy, tick } from 'svelte';
  import { registry, paletteItems, register, activeScope, type PaletteItem } from '$lib/keyboard';

  export let open = false;

  let dialogEl: HTMLDialogElement | null = null;
  let inputEl: HTMLInputElement | null = null;
  let query = '';
  let activeIndex = 0;
  let restoreScope: (() => void) | null = null;

  // Combine registered shortcuts (that have a label) with explicit palette items.
  // For shortcut-derived items we invoke `sc.handler()` with no event — handlers
  // accept an optional KeyboardEvent and must not require it.
  $: items = (() => {
    const list: PaletteItem[] = [];
    for (const sc of $registry) {
      // Skip palette-scoped internal shortcuts and unlabeled.
      if (sc.scope === 'palette') continue;
      if (!sc.label) continue;
      list.push({
        id: `sc:${sc.scope}:${sc.combo}:${sc.label}`,
        label: sc.label,
        group: sc.group ?? 'Shortcuts',
        combo: sc.combo,
        action: () => { sc.handler(); },
      });
    }
    for (const it of $paletteItems) list.push(it);
    return list;
  })();

  $: filtered = (() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((it) => {
      const hay = (it.label + ' ' + (it.group ?? '') + ' ' + (it.combo ?? '')).toLowerCase();
      // Simple substring match — token-AND so multi-word queries narrow.
      return q.split(/\s+/).every((tok) => hay.includes(tok));
    });
  })();

  $: groups = (() => {
    const map = new Map<string, PaletteItem[]>();
    for (const it of filtered) {
      const g = it.group ?? 'Other';
      const arr = map.get(g) ?? [];
      arr.push(it);
      map.set(g, arr);
    }
    return Array.from(map.entries());
  })();

  // Flat ordering matches `filtered` so activeIndex maps directly.
  $: if (activeIndex >= filtered.length) activeIndex = Math.max(0, filtered.length - 1);

  async function openDialog() {
    open = true;
    query = '';
    activeIndex = 0;
    restoreScope = activeScope('palette');
    await tick();
    if (dialogEl && !dialogEl.open) {
      try { dialogEl.showModal(); } catch { /* jsdom fallback */ }
    }
    inputEl?.focus();
  }

  function closeDialog() {
    if (dialogEl?.open) {
      try { dialogEl.close(); } catch { /* ignore */ }
    }
    open = false;
    if (restoreScope) {
      restoreScope();
      restoreScope = null;
    }
  }

  function runAt(idx: number) {
    const it = filtered[idx];
    if (!it) return;
    closeDialog();
    // Run synchronously after closing — handlers (goto, store updates) are
    // safe to invoke here. Wrapping in a microtask added timing variability
    // without buying us anything.
    try { it.action(); } catch { /* swallow — palette must always close cleanly */ }
  }

  function onInputKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      activeIndex = Math.min(filtered.length - 1, activeIndex + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      activeIndex = Math.max(0, activeIndex - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      runAt(activeIndex);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      closeDialog();
    }
  }

  function onBackdropClick(e: MouseEvent) {
    // Native <dialog> reports clicks on the backdrop with target === dialogEl.
    if (e.target === dialogEl) closeDialog();
  }

  // Register the global cmd+k shortcut here so the palette is self-contained.
  let unregOpen: (() => void) | null = null;
  onMount(() => {
    unregOpen = register({
      combo: 'cmd+k',
      scope: 'global',
      label: 'Open command palette',
      group: 'General',
      handler: (e) => {
        e?.preventDefault?.();
        if (open) closeDialog(); else void openDialog();
      },
    });
  });

  onDestroy(() => {
    unregOpen?.();
    if (restoreScope) restoreScope();
  });

  // For tests / external callers.
  export function show() { void openDialog(); }
  export function hide() { closeDialog(); }

  // Track active item index across filtered list for highlighting.
  function indexInFiltered(it: PaletteItem): number {
    return filtered.indexOf(it);
  }
</script>

<dialog
  bind:this={dialogEl}
  class="palette"
  on:click={onBackdropClick}
  on:close={() => { open = false; if (restoreScope) { restoreScope(); restoreScope = null; } }}
  aria-label="Command palette"
>
  <div class="inner" role="document">
    <input
      bind:this={inputEl}
      bind:value={query}
      on:keydown={onInputKey}
      type="text"
      class="search"
      placeholder="Type a command…"
      aria-label="Search commands"
      autocomplete="off"
      spellcheck="false"
    />
    <div class="list" role="listbox" aria-label="Commands">
      {#if filtered.length === 0}
        <div class="empty">No matches</div>
      {:else}
        {#each groups as [groupName, gItems] (groupName)}
          <div class="group-label">{groupName}</div>
          {#each gItems as it (it.id)}
            {@const i = indexInFiltered(it)}
            <button
              type="button"
              class="row"
              class:active={i === activeIndex}
              role="option"
              aria-selected={i === activeIndex}
              on:click={() => runAt(i)}
              on:mouseenter={() => (activeIndex = i)}
            >
              <span class="row-label">{it.label}</span>
              {#if it.combo}<kbd class="row-combo">{it.combo}</kbd>{/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>
    <div class="hint">
      <small>↑↓ navigate · ↵ run · esc close</small>
    </div>
  </div>
</dialog>

<style>
  dialog.palette {
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-elev);
    color: var(--color-fg);
    padding: 0;
    width: min(640px, 92vw);
    max-height: 70vh;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.4);
  }
  dialog.palette::backdrop {
    background: rgba(0, 0, 0, 0.45);
  }
  .inner {
    display: flex;
    flex-direction: column;
    max-height: 70vh;
  }
  .search {
    width: 100%;
    padding: 0.75rem 1rem;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-fg);
    font-size: 0.95rem;
    outline: none;
  }
  .list {
    overflow-y: auto;
    padding: 0.25rem 0;
    flex: 1;
  }
  .group-label {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-fg-muted);
    padding: 0.5rem 1rem 0.25rem;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0.45rem 1rem;
    background: transparent;
    border: none;
    color: var(--color-fg);
    text-align: left;
    cursor: pointer;
    font-size: 0.88rem;
  }
  .row.active {
    background: var(--color-bg-inset);
  }
  .row-label { flex: 1; }
  .row-combo {
    font-family: var(--font-mono, monospace);
    padding: 2px 6px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg-inset);
    font-size: 0.75rem;
    color: var(--color-fg-muted);
  }
  .empty {
    padding: 1rem;
    color: var(--color-fg-muted);
    text-align: center;
    font-size: 0.85rem;
  }
  .hint {
    border-top: 1px solid var(--color-border);
    padding: 0.4rem 1rem;
    color: var(--color-fg-muted);
  }
  .hint small { font-size: 0.72rem; }
</style>
