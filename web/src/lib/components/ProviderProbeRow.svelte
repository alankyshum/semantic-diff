<script lang="ts">
  import type { ProviderProbe } from '$lib/types';

  export let provider: ProviderProbe;

  function truncatePath(p: string | null, max = 48): string {
    if (!p) return '';
    if (p.length <= max) return p;
    // keep tail (more meaningful for binary paths)
    return '…' + p.slice(p.length - (max - 1));
  }
</script>

<div class="provider">
  <div class="provider-name">{provider.name}</div>
  <ul class="binaries">
    {#each provider.binaries as bin}
      <li class="bin" class:found={bin.found} class:missing={!bin.found}>
        <span class="icon" aria-hidden="true">{bin.found ? '✓' : '✗'}</span>
        <span class="bin-name">{bin.name}</span>
        {#if bin.found && bin.version}
          <span class="version">{bin.version}</span>
        {/if}
        {#if bin.found && bin.path}
          <code class="bin-path" title={bin.path}>{truncatePath(bin.path)}</code>
        {:else if !bin.found}
          <span class="not-found">not found</span>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .provider {
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 0.6rem 0.8rem;
    background: var(--color-bg);
    margin-bottom: 0.5rem;
  }
  .provider-name {
    font-weight: 600;
    font-size: 0.9rem;
    margin-bottom: 0.4rem;
    color: var(--color-fg);
    text-transform: capitalize;
  }
  .binaries { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.25rem; }
  .bin {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8rem;
    flex-wrap: wrap;
  }
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.1rem;
    height: 1.1rem;
    border-radius: 50%;
    font-size: 0.75rem;
    font-weight: 700;
    flex-shrink: 0;
  }
  .bin.found .icon { background: var(--color-success); color: var(--color-bg); }
  .bin.missing .icon { background: var(--color-danger); color: var(--color-bg); }
  .bin-name { font-family: monospace; color: var(--color-fg); }
  .version { color: var(--color-fg-muted); font-size: 0.75rem; }
  .bin-path {
    font-family: monospace;
    font-size: 0.72rem;
    color: var(--color-fg-muted);
    background: var(--color-bg-inset);
    padding: 0.05rem 0.35rem;
    border-radius: 3px;
    border: 1px solid var(--color-border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .not-found { color: var(--color-fg-muted); font-style: italic; font-size: 0.75rem; }
</style>
