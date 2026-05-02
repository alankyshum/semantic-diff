<script lang="ts">
  // F17 — Shortcut cheatsheet. Modal listing all currently-registered shortcuts
  // grouped by `group` field. Triggered by `?` (registered globally in
  // +layout.svelte). Press any key to close.
  import { onMount, onDestroy, tick } from 'svelte';
  import { registry, register, activeScope, type Shortcut } from '$lib/keyboard';

  export let open = false;

  let dialogEl: HTMLDialogElement | null = null;
  let restoreScope: (() => void) | null = null;

  $: groups = (() => {
    const map = new Map<string, Shortcut[]>();
    for (const sc of $registry) {
      if (!sc.label) continue;
      if (sc.scope === 'palette') continue;
      const g = sc.group ?? 'Other';
      const arr = map.get(g) ?? [];
      arr.push(sc);
      map.set(g, arr);
    }
    return Array.from(map.entries()).map(([name, items]) => ({
      name,
      items: items.slice().sort((a, b) => a.label.localeCompare(b.label)),
    }));
  })();

  async function show() {
    open = true;
    restoreScope = activeScope('palette');
    await tick();
    if (dialogEl && !dialogEl.open) {
      try { dialogEl.showModal(); } catch { /* jsdom */ }
    }
    dialogEl?.focus();
  }

  function hide() {
    if (dialogEl?.open) {
      try { dialogEl.close(); } catch { /* ignore */ }
    }
    open = false;
    if (restoreScope) {
      restoreScope();
      restoreScope = null;
    }
  }

  function onAnyKey(e: KeyboardEvent) {
    // Modifier-only key events shouldn't dismiss; ignore them so that pressing
    // Shift before another key doesn't insta-close.
    if (e.key === 'Shift' || e.key === 'Control' || e.key === 'Alt' || e.key === 'Meta') return;
    e.preventDefault();
    hide();
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) hide();
  }

  let unregOpen: (() => void) | null = null;
  onMount(() => {
    unregOpen = register({
      combo: '?',
      scope: 'global',
      label: 'Show keyboard shortcuts',
      group: 'General',
      handler: () => { if (open) hide(); else void show(); },
    });
  });

  onDestroy(() => {
    unregOpen?.();
    if (restoreScope) restoreScope();
  });

  export function open_() { void show(); }
  export function close_() { hide(); }
</script>

<dialog
  bind:this={dialogEl}
  class="cheatsheet"
  on:click={onBackdropClick}
  on:keydown={onAnyKey}
  on:close={() => { open = false; if (restoreScope) { restoreScope(); restoreScope = null; } }}
  aria-label="Keyboard shortcuts"
  tabindex="-1"
>
  <div class="inner">
    <header class="head">
      <h2>Keyboard shortcuts</h2>
    </header>
    <div class="body">
      {#if groups.length === 0}
        <div class="empty">No shortcuts registered.</div>
      {:else}
        {#each groups as g (g.name)}
          <section class="group">
            <h3>{g.name}</h3>
            <dl>
              {#each g.items as sc (sc.combo + ':' + sc.label)}
                <div class="row">
                  <dt><kbd>{sc.combo}</kbd></dt>
                  <dd>{sc.label}</dd>
                </div>
              {/each}
            </dl>
          </section>
        {/each}
      {/if}
    </div>
    <footer class="foot">
      <small style="color: var(--color-fg-muted)">Press any key to close</small>
    </footer>
  </div>
</dialog>

<style>
  dialog.cheatsheet {
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-elev);
    color: var(--color-fg);
    padding: 0;
    width: min(560px, 92vw);
    max-height: 80vh;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.4);
  }
  dialog.cheatsheet::backdrop { background: rgba(0, 0, 0, 0.45); }

  .inner { display: flex; flex-direction: column; max-height: 80vh; }
  .head {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--color-border);
  }
  .head h2 { margin: 0; font-size: 1rem; }
  .body {
    overflow-y: auto;
    padding: 0.75rem 1rem;
    flex: 1;
  }
  .group + .group { margin-top: 1rem; }
  .group h3 {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-fg-muted);
    margin: 0 0 0.4rem;
  }
  dl { margin: 0; }
  .row {
    display: grid;
    grid-template-columns: 8em 1fr;
    align-items: center;
    gap: 0.5rem;
    padding: 0.2rem 0;
  }
  dt { margin: 0; }
  dd { margin: 0; font-size: 0.88rem; }
  kbd {
    font-family: var(--font-mono, monospace);
    padding: 2px 6px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg-inset);
    font-size: 0.78rem;
    color: var(--color-fg);
  }
  .foot {
    padding: 0.5rem 1rem;
    border-top: 1px solid var(--color-border);
    text-align: right;
  }
  .empty { color: var(--color-fg-muted); padding: 1rem; }
</style>
