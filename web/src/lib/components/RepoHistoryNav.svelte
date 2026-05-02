<script lang="ts">
  import type { ResultSummary } from '$lib/types';
  import { formatDate, statusColor } from '$lib/util/date';

  export let repoName: string;
  export let currentId: string;

  let open = false;
  let loading = false;
  let error: string | null = null;
  let items: ResultSummary[] = [];
  let buttonEl: HTMLButtonElement | null = null;
  let panelEl: HTMLDivElement | null = null;

  // Track the last repoName we fetched for, so the reactive trigger doesn't
  // re-fetch needlessly on unrelated reactivity passes.
  let lastFetchedFor: string | null = null;

  async function loadHistory(name: string) {
    loading = true;
    error = null;
    // Mark immediately so a re-entrant reactive trigger for the same name
    // can't kick off a second fetch while this one is in flight.
    lastFetchedFor = name;
    try {
      const res = await fetch(`/api/repos/${encodeURIComponent(name)}/results`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = (await res.json()) as ResultSummary[];
      items = Array.isArray(data) ? data : [];
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      items = [];
    } finally {
      loading = false;
    }
  }

  // Fetch on mount and whenever repoName changes. Svelte runs `$:` blocks
  // once during initialization and again on every dep change, so this also
  // covers the on-mount case.
  $: if (repoName && repoName !== lastFetchedFor) {
    void loadHistory(repoName);
  }

  function toggle() {
    // Inline the disabled check rather than reading the `$:` reactive
    // `disabled`/`count` — reactive declarations are not guaranteed to be
    // up-to-date when read from a non-reactive event handler.
    if (loading || (error === null && items.length <= 1)) return;
    open = !open;
  }

  function close() {
    open = false;
  }

  function onItemClick() {
    open = false;
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      open = false;
      buttonEl?.focus();
    }
  }

  function onWindowClick(e: MouseEvent) {
    if (!open) return;
    const t = e.target as Node | null;
    if (!t) return;
    if (panelEl?.contains(t)) return;
    if (buttonEl?.contains(t)) return;
    open = false;
  }

  function truncate(s: string, max = 40): string {
    if (!s) return '';
    return s.length > max ? s.slice(0, max - 1) + '…' : s;
  }

  $: count = items.length;
  // Disabled while loading (no items to show yet) or when there are <=1 items
  // and no error. In the error case the button stays clickable so the user
  // can open the panel and read the error message.
  $: disabled = loading || (error === null && count <= 1);
  $: countLabel = error ? '?' : loading ? '…' : String(count);
  $: currentItem = items.find((i) => i.id === currentId) ?? null;
  $: orderedItems = items; // server returns by recency already
</script>

<svelte:window on:keydown={onWindowKeydown} on:mousedown={onWindowClick} />

<div class="repo-history-nav">
  <button
    bind:this={buttonEl}
    type="button"
    class="trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    {disabled}
    title={error ? `Failed to load history: ${error}` : `History for ${repoName}`}
    on:click={toggle}
  >
    <span class="label">History ({countLabel})</span>
    <span aria-hidden="true" class="caret">▾</span>
  </button>

  {#if open}
    <div
      bind:this={panelEl}
      class="panel"
      role="menu"
      aria-label="Repo review history"
    >
      {#if error}
        <div class="state error" role="alert">Failed to load history: {error}</div>
      {:else if loading}
        <div class="state">Loading…</div>
      {:else if orderedItems.length === 0}
        <div class="state empty">No reviews for this repo.</div>
      {:else}
        {#each orderedItems as item (item.id)}
          {#if item.id === currentId}
            <div
              class="row current"
              role="menuitem"
              aria-current="true"
              tabindex="-1"
            >
              <span
                class="status-badge"
                style="background: {statusColor(item.status)}"
                title={item.status}
              >
                {item.status}
              </span>
              <span class="title" title={item.title}>{truncate(item.title)}</span>
              <span class="time">{formatDate(item.created_at)}</span>
            </div>
          {:else}
            <a
              class="row"
              href={`/r/${item.id}`}
              role="menuitem"
              on:click={onItemClick}
            >
              <span
                class="status-badge"
                style="background: {statusColor(item.status)}"
                title={item.status}
              >
                {item.status}
              </span>
              <span class="title" title={item.title}>{truncate(item.title)}</span>
              <span class="time">{formatDate(item.created_at)}</span>
            </a>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .repo-history-nav {
    position: relative;
    display: inline-block;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    background: transparent;
    color: var(--color-fg);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 0.25rem 0.55rem;
    font-size: 0.8rem;
    line-height: 1;
    height: 28px;
    cursor: pointer;
    transition: background 0.1s, border-color 0.1s;
  }
  .trigger:hover:not(:disabled) {
    background: var(--color-bg-inset);
    border-color: var(--color-accent);
  }
  .trigger:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .trigger:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
  .caret {
    color: var(--color-fg-muted);
    font-size: 0.7rem;
  }

  .panel {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    min-width: 360px;
    max-height: 60vh;
    overflow-y: auto;
    background: var(--color-bg-elev, var(--color-bg));
    border: 1px solid var(--color-border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    padding: 0.25rem;
  }

  .row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.55rem;
    border-radius: 4px;
    color: var(--color-fg);
    text-decoration: none;
    font-size: 0.82rem;
  }
  a.row:hover {
    background: var(--color-bg-inset);
    text-decoration: none;
  }
  .row.current {
    background: var(--color-bg-inset);
    cursor: default;
    font-weight: 600;
  }

  .status-badge {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    /* the background color is set inline; the text inside is hidden */
    overflow: hidden;
    text-indent: -9999px;
    flex-shrink: 0;
  }

  .title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .time {
    color: var(--color-fg-muted);
    font-size: 0.72rem;
    white-space: nowrap;
  }

  .state {
    padding: 0.6rem 0.6rem;
    font-size: 0.8rem;
    color: var(--color-fg-muted);
  }
  .state.error {
    color: var(--color-danger);
  }
</style>
