<script lang="ts">
  import type { Group, GroupReview } from '$lib/types';

  export let groups: Group[];
  export let reviews: Record<string, GroupReview>;
  export let selectedGroupId: string;

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
    if (!s) return '#8b949e';
    switch (s.state) {
      case 'ready': return '#3fb950';
      case 'loading': return '#d29922';
      case 'error': return '#f85149';
      case 'skipped': return '#8b949e';
      default: return '#8b949e';
    }
  }
</script>

<aside class="sidebar">
  <div class="sidebar-header">Groups</div>
  {#each groups as group}
    <button
      class="group-item"
      class:selected={group.id === selectedGroupId}
      on:click={() => selectedGroupId = group.id}
    >
      <div class="group-label">{group.label}</div>
      <div class="section-dots">
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
    width: 220px; min-width: 180px;
    border-right: 1px solid #30363d;
    background: #0d1117;
    padding: 0.75rem 0;
    overflow-y: auto;
  }
  .sidebar-header {
    font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.1em;
    color: #8b949e; padding: 0 0.75rem 0.5rem;
  }
  .group-item {
    display: block; width: 100%; text-align: left;
    padding: 0.5rem 0.75rem; border: none; background: none;
    color: #e6edf3; cursor: pointer; transition: background 0.1s;
  }
  .group-item:hover { background: #161b22; }
  .group-item.selected { background: #21262d; border-left: 3px solid #58a6ff; padding-left: calc(0.75rem - 3px); }
  .group-label { font-size: 0.875rem; margin-bottom: 0.2rem; word-break: break-word; }
  .section-dots { display: flex; gap: 0.3rem; font-size: 0.65rem; }
  .dot { cursor: help; }
</style>
