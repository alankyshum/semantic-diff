<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Group, GroupReview } from '$lib/types';

  export let groups: Group[];
  export let reviews: Record<string, GroupReview>;
  export let selectedGroupId: string;
  export let collapsed: boolean = false;

  const dispatch = createEventDispatcher<{ toggleCollapsed: boolean }>();

  function sectionDot(review: GroupReview | undefined, section: string): string {
    const s = review?.sections?.[section];
    if (!s) return '○';
    switch (s.state) {
      case 'ready': return '●';
      case 'loading': return '◌';
      case 'error': return '✗';
      case 'skipped': return '–';
      default: return '○';
    }
  }

  function dotColor(review: GroupReview | undefined, section: string): string {
    const s = review?.sections?.[section];
    if (!s) return 'var(--color-fg-muted)';
    switch (s.state) {
      case 'ready': return 'var(--color-success)';
      case 'loading': return 'var(--color-warning)';
      case 'error': return 'var(--color-danger)';
      case 'skipped': return 'var(--color-fg-muted)';
      default: return 'var(--color-fg-muted)';
    }
  }
</script>

<aside class="sidebar" class:collapsed>
  <div class="sidebar-top">
    {#if !collapsed}
      <div class="sidebar-header">Groups</div>
    {/if}
    <button
      type="button"
      class="collapse-btn"
      aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
      title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
      on:click={() => dispatch('toggleCollapsed', !collapsed)}
    >
      {collapsed ? '⟩' : '⟨'}
    </button>
  </div>
  {#each groups as group}
    <button
      class="group-item"
      class:selected={group.id === selectedGroupId}
      on:click={() => selectedGroupId = group.id}
      title={group.label}
    >
      {#if !collapsed}
        <div class="group-label">{group.label}</div>
      {/if}
      <div class="section-dots" class:vertical={collapsed}>
        {#each ['WHY', 'WHAT', 'HOW', 'VERDICT'] as sec}
          <span
            class="dot"
            style="color: {dotColor(reviews[group.id], sec)}"
            title="{sec}: {reviews[group.id]?.sections?.[sec]?.state ?? 'pending'}"
          >
            {sectionDot(reviews[group.id], sec)}
          </span>
        {/each}
      </div>
    </button>
  {/each}
</aside>

<style>
  .sidebar {
    width: 100%;
    min-width: 0;
    border-right: 1px solid var(--color-border);
    background: var(--color-bg);
    padding: 0.75rem 0;
    position: sticky;
    top: var(--header-h);
    height: calc(100vh - var(--header-h));
    overflow-y: auto;
    align-self: start;
  }
  .sidebar-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0.5rem 0.5rem;
    gap: 0.25rem;
  }
  .sidebar-header {
    font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.1em;
    color: var(--color-fg-muted);
    padding-left: 0.25rem;
  }
  .collapse-btn {
    background: transparent;
    border: 1px solid var(--color-border);
    color: var(--color-fg-muted);
    border-radius: 4px;
    width: 24px;
    height: 24px;
    line-height: 1;
    font-size: 0.75rem;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .collapse-btn:hover { color: var(--color-fg); border-color: var(--color-accent); }
  .group-item {
    display: block; width: 100%; text-align: left;
    padding: 0.5rem 0.75rem; border: none; background: none;
    color: var(--color-fg); cursor: pointer; transition: background 0.1s;
    scroll-margin-block: 1rem;
  }
  .group-item:hover { background: var(--color-bg-elev); }
  .group-item.selected {
    background: var(--color-bg-inset);
    border-left: 3px solid var(--color-accent);
    padding-left: calc(0.75rem - 3px);
  }
  .group-label { font-size: 0.875rem; margin-bottom: 0.2rem; word-break: break-word; }
  .section-dots { display: flex; gap: 0.3rem; font-size: 0.65rem; }
  .section-dots.vertical { flex-direction: column; align-items: center; gap: 0.15rem; }
  .dot { cursor: help; }

  .sidebar.collapsed .group-item {
    padding: 0.5rem 0;
    text-align: center;
  }
  .sidebar.collapsed .group-item.selected {
    padding-left: 0;
    border-left: none;
    border-right: 3px solid var(--color-accent);
  }

  /* Mobile: sidebar stacks above main; no sticky. */
  @media (max-width: 768px) {
    .sidebar {
      position: static;
      height: auto;
      max-height: 40vh;
      border-right: none;
      border-bottom: 1px solid var(--color-border);
    }
  }
</style>
