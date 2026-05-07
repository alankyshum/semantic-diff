<script lang="ts">
  /**
   * Ghost-flashing skeleton mirroring the review page shell.
   * Used while the document is loading or while groups are still being
   * computed — gives the user the same shimmer feedback that section
   * cards use during inflight section rendering.
   *
   * `variant`:
   *   - `page`  : full page-loading skeleton (header, sidebar, main).
   *   - `sidebar-only` : just placeholder group items (used when doc is
   *                     loaded but `groups` is still empty).
   *   - `main-only`    : section-card placeholders.
   */
  export let variant: 'page' | 'sidebar-only' | 'main-only' = 'page';
  /** Number of placeholder rows for sidebar/main lists. */
  export let rows: number = 4;
</script>

{#if variant === 'page'}
  <div class="skel-layout" aria-busy="true" aria-label="Loading review">
    <div class="skel-header">
      <div class="skel-bar skel-bar--lg" style="width: 220px"></div>
      <div class="skel-bar" style="width: 80px"></div>
      <div class="skel-spacer"></div>
      <div class="skel-bar" style="width: 60px"></div>
      <div class="skel-bar" style="width: 60px"></div>
    </div>
    <div class="skel-body">
      <div class="skel-sidebar">
        <div class="skel-bar" style="width: 60%; margin: 0.5rem 0.75rem"></div>
        {#each Array.from({ length: rows }) as _, i (i)}
          <div class="skel-group">
            <div class="skel-bar" style="width: 80%"></div>
            <div class="skel-dots">
              {#each Array.from({ length: 4 }) as _d, di (di)}
                <span class="skel-dot"></span>
              {/each}
            </div>
          </div>
        {/each}
      </div>
      <div class="skel-main">
        <div class="skel-bar skel-bar--lg" style="width: 50%"></div>
        <div class="skel-bar" style="width: 70%"></div>
        <div class="skel-tabs">
          <div class="skel-bar" style="width: 60px"></div>
          <div class="skel-bar" style="width: 80px"></div>
        </div>
        {#each ['WHY', 'WHAT', 'HOW', 'VERDICT'] as label}
          <div class="skel-card">
            <div class="skel-bar" style="width: 80px; margin-bottom: 0.6rem"></div>
            <div class="skel-bar" style="height: 80px; width: 100%"></div>
          </div>
        {/each}
      </div>
    </div>
  </div>
{:else if variant === 'sidebar-only'}
  <div class="skel-sidebar-only" aria-busy="true" aria-label="Loading groups">
    {#each Array.from({ length: rows }) as _, i (i)}
      <div class="skel-group">
        <div class="skel-bar" style="width: 80%"></div>
        <div class="skel-dots">
          {#each Array.from({ length: 4 }) as _d, di (di)}
            <span class="skel-dot"></span>
          {/each}
        </div>
      </div>
    {/each}
  </div>
{:else}
  <div class="skel-main" aria-busy="true" aria-label="Loading content">
    {#each Array.from({ length: rows }) as _, i (i)}
      <div class="skel-card">
        <div class="skel-bar" style="width: 80px; margin-bottom: 0.6rem"></div>
        <div class="skel-bar" style="height: 80px; width: 100%"></div>
      </div>
    {/each}
  </div>
{/if}

<style>
  /* Shimmer keyframes shared with the per-section skeleton in +page.svelte. */
  @keyframes shimmer {
    0%   { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }

  /* Reusable bar — same gradient as `.skeleton` in +page.svelte. */
  .skel-bar {
    height: 14px;
    border-radius: 4px;
    background: linear-gradient(
      90deg,
      var(--color-bg-inset) 25%,
      var(--color-border) 50%,
      var(--color-bg-inset) 75%
    );
    background-size: 200% 100%;
    animation: shimmer 1.5s infinite;
  }
  .skel-bar--lg { height: 22px; }

  .skel-layout {
    display: flex;
    flex-direction: column;
    min-height: calc(100vh - var(--app-nav-h));
  }
  .skel-header {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    padding: 0.75rem 1.5rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-elev);
    height: var(--header-h);
  }
  .skel-spacer { flex: 1; }
  .skel-body {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    flex: 1;
    min-height: 0;
  }
  .skel-sidebar {
    border-right: 1px solid var(--color-border);
    background: var(--color-bg);
    padding: 0.75rem 0;
  }
  .skel-sidebar-only { padding: 0.5rem 0; }
  .skel-group {
    padding: 0.5rem 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .skel-dots { display: flex; gap: 0.4rem; }
  .skel-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: linear-gradient(
      90deg,
      var(--color-bg-inset) 25%,
      var(--color-border) 50%,
      var(--color-bg-inset) 75%
    );
    background-size: 200% 100%;
    animation: shimmer 1.5s infinite;
  }
  .skel-main {
    padding: 1.5rem;
    flex: 1;
    width: 100%;
    max-width: var(--content-max);
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .skel-tabs {
    display: flex;
    gap: 0.5rem;
    border-bottom: 1px solid var(--color-border);
    padding-bottom: 0.4rem;
  }
  .skel-card {
    background: var(--color-bg-elev);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 1.25rem;
    margin-bottom: 0.5rem;
  }

  @media (max-width: 768px) {
    .skel-body { grid-template-columns: 1fr; }
  }
</style>
