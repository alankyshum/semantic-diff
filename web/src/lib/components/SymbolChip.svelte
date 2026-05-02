<script lang="ts">
  export let file: string;
  export let line: number | null = null;
  export let repoUrl: string | null = null;

  function basename(path: string): string {
    const i = path.lastIndexOf('/');
    return i >= 0 ? path.slice(i + 1) : path;
  }

  function stripGit(url: string): string {
    return url.endsWith('.git') ? url.slice(0, -4) : url;
  }

  $: label = line != null ? `${basename(file)}:${line}` : basename(file);
  $: titleAttr = line != null ? `${file}:${line}` : file;
  $: href = repoUrl
    ? `${stripGit(repoUrl)}/blob/HEAD/${file}${line != null ? `#L${line}` : ''}`
    : null;
</script>

{#if href}
  <a
    class="symbol-chip"
    href={href}
    title={titleAttr}
    target="_blank"
    rel="noopener noreferrer"
  >{label}</a>
{:else}
  <span class="symbol-chip" title={titleAttr}>{label}</span>
{/if}

<style>
  .symbol-chip {
    display: inline;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85em;
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    padding: 0.05em 0.4em;
    border-radius: 4px;
    text-decoration: none;
  }
  a.symbol-chip:hover { color: var(--color-accent); border-color: var(--color-accent); }
</style>
