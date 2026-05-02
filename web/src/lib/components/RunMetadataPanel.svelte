<script lang="ts">
  import type { RunMetadata, RepoInfo } from '$lib/types';
  import { formatDate, formatDuration } from '$lib/util/date';

  export let metadata: RunMetadata;
  export let repo: RepoInfo | undefined = undefined;

  let copied = false;
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  function copyArgv() {
    const text = metadata.cli_argv.join(' ');
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(text).then(() => {
        copied = true;
        if (copyTimer) clearTimeout(copyTimer);
        copyTimer = setTimeout(() => { copied = false; }, 1500);
      }).catch(() => { /* ignore */ });
    }
  }

  function shortSha(sha: string | undefined): string {
    return sha ? sha.slice(0, 7) : '–';
  }

  $: argv = metadata.cli_argv?.join(' ') ?? '';

  // Per-section timing aggregation: total ms by section.
  interface SectionAgg { section: string; total_ms: number; count: number; cache_hits: number; }
  $: sectionTotals = (() => {
    const by = new Map<string, SectionAgg>();
    for (const t of metadata.timings ?? []) {
      const cur = by.get(t.section) ?? { section: t.section, total_ms: 0, count: 0, cache_hits: 0 };
      cur.total_ms += t.duration_ms;
      cur.count += 1;
      if (t.cache_hit) cur.cache_hits += 1;
      by.set(t.section, cur);
    }
    return Array.from(by.values()).sort((a, b) => b.total_ms - a.total_ms);
  })();
</script>

<div class="meta-panel">
  <dl class="grid">
    <dt>Tool version</dt>
    <dd>{metadata.tool_version}</dd>

    <dt>Schema</dt>
    <dd>v{metadata.schema_version}</dd>

    <dt>Started</dt>
    <dd>{formatDate(metadata.started_at)}</dd>

    <dt>Completed</dt>
    <dd>{metadata.completed_at ? formatDate(metadata.completed_at) : 'running…'}</dd>

    <dt>Total duration</dt>
    <dd>{formatDuration(metadata.total_duration_ms)}</dd>

    <dt>Working dir</dt>
    <dd><code>{metadata.working_dir}</code></dd>

    <dt>CLI argv</dt>
    <dd class="argv-row">
      <code class="argv">{argv}</code>
      <button type="button" class="copy-btn" on:click={copyArgv} aria-label="Copy CLI argv">
        {copied ? 'Copied!' : 'Copy'}
      </button>
    </dd>
  </dl>

  {#if metadata.llm}
    <h4>LLM</h4>
    <dl class="grid">
      <dt>Provider</dt><dd>{metadata.llm.provider}</dd>
      <dt>Model</dt><dd>{metadata.llm.model ?? '–'}</dd>
      <dt>CLI path</dt><dd><code>{metadata.llm.cli_path ?? '–'}</code></dd>
      <dt>CLI version</dt><dd>{metadata.llm.cli_version ?? '–'}</dd>
    </dl>
  {/if}

  {#if repo}
    <h4>Repo</h4>
    <dl class="grid">
      <dt>Name</dt><dd>{repo.name ?? '–'}</dd>
      <dt>Branch</dt><dd>{repo.branch ?? '–'}</dd>
      <dt>HEAD</dt><dd><code>{shortSha(repo.head_sha)}</code></dd>
      <dt>Remote</dt>
      <dd>
        {#if repo.remote_url}
          <a href={repo.remote_url} target="_blank" rel="noopener noreferrer">{repo.remote_url} ↗</a>
        {:else}
          –
        {/if}
      </dd>
      <dt>Root</dt><dd><code>{repo.root_path ?? '–'}</code></dd>
    </dl>
  {/if}

  {#if metadata.tokens}
    <h4>Tokens</h4>
    <dl class="grid">
      <dt>Input</dt><dd>{metadata.tokens.input_tokens ?? '–'}</dd>
      <dt>Output</dt><dd>{metadata.tokens.output_tokens ?? '–'}</dd>
      <dt>Cost</dt>
      <dd>{metadata.tokens.cost_usd != null ? `$${metadata.tokens.cost_usd.toFixed(4)}` : '–'}</dd>
    </dl>
  {/if}

  {#if metadata.skill_files?.length}
    <h4>Skill files ({metadata.skill_files.length})</h4>
    <ul class="skills">
      {#each metadata.skill_files as s}
        <li>
          <code class="skill-name">{s.name}</code>
          <span class="skill-hash" title={s.hash_blake3}>{s.hash_blake3.slice(0, 12)}</span>
          <code class="skill-path">{s.path}</code>
        </li>
      {/each}
    </ul>
  {/if}

  {#if sectionTotals.length}
    <h4>Per-section timings</h4>
    <table class="timings">
      <thead>
        <tr><th>Section</th><th>Total</th><th>Runs</th><th>Cache hits</th></tr>
      </thead>
      <tbody>
        {#each sectionTotals as t}
          <tr>
            <td>{t.section}</td>
            <td>{formatDuration(t.total_ms)}</td>
            <td>{t.count}</td>
            <td>{t.cache_hits}/{t.count}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .meta-panel {
    font-family: var(--font-mono, 'SFMono-Regular', 'Consolas', monospace);
    font-size: 0.8rem;
    color: var(--color-fg);
  }
  h4 {
    margin: 1rem 0 0.4rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-fg-muted);
  }
  dl.grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.25rem 1rem;
    margin: 0;
  }
  dt { color: var(--color-fg-muted); }
  dd { margin: 0; word-break: break-all; }
  code {
    font-family: inherit;
    background: var(--color-bg-inset);
    padding: 0.05rem 0.3rem;
    border-radius: 3px;
  }
  .argv-row { display: flex; gap: 0.5rem; align-items: flex-start; }
  .argv { flex: 1; white-space: pre-wrap; }
  .copy-btn {
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0.15rem 0.5rem;
    font-size: 0.7rem;
    color: var(--color-fg);
    cursor: pointer;
    flex-shrink: 0;
  }
  .copy-btn:hover { border-color: var(--color-accent); }
  ul.skills { list-style: none; padding: 0; margin: 0; }
  ul.skills li { display: flex; gap: 0.5rem; align-items: center; padding: 0.15rem 0; flex-wrap: wrap; }
  .skill-hash { color: var(--color-fg-muted); font-size: 0.75rem; }
  .skill-path { color: var(--color-fg-muted); font-size: 0.75rem; }
  table.timings {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.78rem;
  }
  table.timings th, table.timings td {
    text-align: left;
    padding: 0.25rem 0.5rem;
    border-bottom: 1px solid var(--color-border);
  }
  table.timings th { color: var(--color-fg-muted); font-weight: 500; }
</style>
