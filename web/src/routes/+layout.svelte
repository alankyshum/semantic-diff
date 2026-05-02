<script lang="ts">
  import '$lib/styles/tokens.css';
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { effectiveTheme, applyTheme } from '$lib/stores/theme';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import CommandPalette from '$lib/components/CommandPalette.svelte';
  import ShortcutCheatsheet from '$lib/components/ShortcutCheatsheet.svelte';
  import { register, dispatch, currentScope, type Scope } from '$lib/keyboard';

  onMount(() => {
    const unsub = effectiveTheme.subscribe((t) => applyTheme(t));
    return () => unsub();
  });

  // Hide gear icon on /settings page itself.
  $: onSettingsPage = $page.url.pathname === '/settings';

  // --- Scope tracking based on route id ---
  // Palette/cheatsheet temporarily set 'palette' themselves; we only set the
  // base page scope here when the palette isn't active.
  $: routeId = $page.route.id ?? '';
  $: baseScope = (routeId === '/' ? 'index'
    : routeId === '/r/[id]' ? 'review-detail'
    : routeId === '/r/[id]/issues' ? 'review-detail'
    : 'global') as Scope;

  $: if (typeof window !== 'undefined' && $currentScope !== 'palette') {
    currentScope.set(baseScope);
  }

  // --- Global keydown listener ---
  function onKeydown(e: KeyboardEvent) {
    // Use the live scope from the store (palette overrides base) — `$currentScope`
    // gives reactive access without subscribing per keystroke.
    const fired = dispatch(e, $currentScope);
    if (fired) e.preventDefault();
  }

  // --- Register global shortcuts ---
  // (cmd+k and ? are registered by the palette/cheatsheet components themselves.)
  let unregs: Array<() => void> = [];

  onMount(() => {
    unregs.push(register({
      combo: 'g h',
      scope: 'global',
      label: 'Go to home',
      group: 'Navigation',
      handler: () => { void goto('/'); },
    }));
    unregs.push(register({
      combo: 'g s',
      scope: 'global',
      label: 'Go to settings',
      group: 'Navigation',
      handler: () => { void goto('/settings'); },
    }));
    unregs.push(register({
      combo: 'g i',
      scope: 'review-detail',
      label: 'Go to issues',
      group: 'Navigation',
      // /r/[id]/issues was shipped in F13.
      handler: () => {
        // Only meaningful on review-detail; the scope filter already gates this.
        const id = $page.params.id;
        if (id) void goto(`/r/${id}/issues`);
      },
    }));

    if (typeof document !== 'undefined') {
      document.addEventListener('keydown', onKeydown, true);
    }
  });

  onDestroy(() => {
    for (const u of unregs) u();
    if (typeof document !== 'undefined') {
      document.removeEventListener('keydown', onKeydown, true);
    }
  });
</script>

<svelte:head>
  <title>semantic-diff</title>
</svelte:head>

<nav class="app-nav" aria-label="Primary">
  <div class="nav-left">
    <a href="/" class="wordmark">semantic-diff</a>
  </div>
  <div class="nav-right">
    {#if !onSettingsPage}
      <a href="/settings" class="icon-link" title="Settings" aria-label="Settings">
        <span aria-hidden="true">⚙</span>
      </a>
    {/if}
    <ThemeToggle />
  </div>
</nav>

<slot />

<CommandPalette />
<ShortcutCheatsheet />

<style>
  .app-nav {
    position: sticky;
    top: 0;
    z-index: 100;
    height: var(--app-nav-h);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1rem;
    background: var(--color-bg-elev);
    border-bottom: 1px solid var(--color-border);
  }
  .nav-left, .nav-right { display: flex; align-items: center; gap: 0.5rem; }
  .wordmark {
    color: var(--color-fg);
    font-weight: 600;
    font-size: 0.9rem;
    letter-spacing: 0.01em;
  }
  .wordmark:hover { color: var(--color-accent); text-decoration: none; }
  .icon-link {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    font-size: 1.05rem;
    line-height: 1;
  }
  .icon-link:hover {
    background: var(--color-bg-inset);
    border-color: var(--color-accent);
    text-decoration: none;
  }
  .icon-link:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
</style>
