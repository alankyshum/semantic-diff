<script lang="ts">
  import '$lib/styles/tokens.css';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { effectiveTheme, applyTheme } from '$lib/stores/theme';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';

  onMount(() => {
    const unsub = effectiveTheme.subscribe((t) => applyTheme(t));
    return () => unsub();
  });

  // Hide gear icon on /settings page itself.
  $: onSettingsPage = $page.url.pathname === '/settings';
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
