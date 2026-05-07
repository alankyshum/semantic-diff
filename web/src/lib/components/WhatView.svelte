<script lang="ts">
  import { onMount } from 'svelte';
  import { effectiveTheme } from '$lib/stores/theme';
  import MarkdownView from './MarkdownView.svelte';

  export let content: string;

  type WhatRow = { component: string; before: string; after: string; risk: string };

  let rows: WhatRow[] = [];
  let chartJson = '';
  let chartContainer: HTMLElement | null = null;
  let parseError = '';
  let sortCol: keyof WhatRow | null = null;
  let sortAsc = true;

  const RISK_ORDER: Record<string, number> = { high: 3, medium: 2, low: 1, none: 0 };

  /**
   * Extract a balanced JSON array (or object) starting at the first `[`/`{`
   * in `raw`, respecting string literals and escapes. The previous regex
   * `/\[[\s\S]*?\]/` was non-greedy and broke whenever a string value
   * contained a `]` (e.g. `"warnings: []"`), truncating the JSON mid-string.
   */
  function extractBalancedJson(raw: string): string | null {
    // Find the first opening bracket of either kind.
    let start = -1;
    let openCh = '';
    let closeCh = '';
    for (let i = 0; i < raw.length; i++) {
      const c = raw[i];
      if (c === '[' || c === '{') {
        start = i;
        openCh = c;
        closeCh = c === '[' ? ']' : '}';
        break;
      }
    }
    if (start < 0) return null;

    let depth = 0;
    let inStr = false;
    let strQuote = '';
    let escaped = false;
    for (let i = start; i < raw.length; i++) {
      const c = raw[i];
      if (inStr) {
        if (escaped) { escaped = false; continue; }
        if (c === '\\') { escaped = true; continue; }
        if (c === strQuote) { inStr = false; }
        continue;
      }
      if (c === '"' || c === "'") { inStr = true; strQuote = c; continue; }
      if (c === openCh) depth++;
      else if (c === closeCh) {
        depth--;
        if (depth === 0) return raw.slice(start, i + 1);
      }
    }
    return null;
  }

  function parse(raw: string) {
    parseError = '';
    rows = [];
    chartJson = '';

    const chartMatch = raw.match(/```chart\n([\s\S]*?)```/);
    if (chartMatch) chartJson = chartMatch[1].trim();

    // Strip the chart fence before locating the JSON array so a `]` inside the
    // chart JSON config can't truncate the WHAT array.
    const withoutChart = chartMatch
      ? raw.slice(0, chartMatch.index!) + raw.slice(chartMatch.index! + chartMatch[0].length)
      : raw;

    const jsonText = extractBalancedJson(withoutChart);
    if (jsonText) {
      try {
        const parsed = JSON.parse(jsonText);
        if (Array.isArray(parsed)) {
          rows = parsed.map((r: any) => ({
            component: String(r.component ?? ''),
            before: String(r.before ?? ''),
            after: String(r.after ?? ''),
            risk: String(r.risk ?? 'none').toLowerCase(),
          }));
        }
      } catch (e) {
        parseError = `JSON parse error: ${e}`;
      }
    }
  }

  function sort(col: keyof WhatRow) {
    if (sortCol === col) {
      sortAsc = !sortAsc;
    } else {
      sortCol = col;
      sortAsc = true;
    }
    rows = [...rows].sort((a, b) => {
      let cmp: number;
      if (col === 'risk') {
        cmp = (RISK_ORDER[a.risk] ?? 0) - (RISK_ORDER[b.risk] ?? 0);
      } else {
        cmp = a[col].localeCompare(b[col]);
      }
      return sortAsc ? cmp : -cmp;
    });
  }

  function riskClass(r: string): string {
    if (r === 'high') return 'risk-high';
    if (r === 'medium') return 'risk-medium';
    if (r === 'low') return 'risk-low';
    return '';
  }

  async function renderChart(json: string, dark: boolean) {
    if (!json || !chartContainer) return;
    const { renderJsonChart } = await import('$lib/util/charts');
    const { element, error } = renderJsonChart(json, dark);
    chartContainer.innerHTML = '';
    if (error) {
      chartContainer.innerHTML = `<div class="chart-error">${error}</div>`;
    } else {
      chartContainer.appendChild(element);
    }
  }

  $: parse(content);
  $: if (chartJson && chartContainer) renderChart(chartJson, $effectiveTheme === 'dark');
</script>

{#if rows.length > 0}
  <div class="what-table-wrap">
    <table class="what-table">
      <thead>
        <tr>
          {#each ['component', 'before', 'after', 'risk'] as col}
            <th
              class:sortable={true}
              class:sorted={sortCol === col}
              on:click={() => sort(col as keyof WhatRow)}
            >
              {col.charAt(0).toUpperCase() + col.slice(1)}
              {#if sortCol === col}
                <span class="sort-arrow">{sortAsc ? '▲' : '▼'}</span>
              {/if}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each rows as row}
          <tr>
            <td class="col-component"><code>{row.component}</code></td>
            <td>{row.before}</td>
            <td>{row.after}</td>
            <td class={riskClass(row.risk)}>{row.risk}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#if chartJson}
    <div class="what-chart" bind:this={chartContainer}></div>
  {/if}
{:else if parseError}
  <div class="what-error">{parseError}</div>
  <MarkdownView {content} />
{:else}
  <!-- Fallback: LLM returned markdown instead of JSON -->
  <MarkdownView {content} />
{/if}

<style>
  .what-table-wrap { overflow-x: auto; margin-bottom: 0.75rem; }
  .what-table {
    border-collapse: collapse; width: 100%; font-size: 0.875rem;
  }
  .what-table th, .what-table td {
    padding: 0.45rem 0.75rem; border: 1px solid var(--color-border); text-align: left;
  }
  .what-table th {
    background: var(--color-bg-inset); font-weight: 600; cursor: pointer;
    user-select: none; white-space: nowrap;
  }
  .what-table th:hover { background: var(--color-bg-elev); }
  .what-table tr:nth-child(even) { background: var(--color-bg-elev); }
  .col-component code {
    background: var(--color-bg-inset); border-radius: 3px; padding: 0.1em 0.3em;
    font-size: 0.85em;
  }
  .sort-arrow { font-size: 0.65rem; margin-left: 2px; opacity: 0.7; }
  .sortable { position: relative; }

  :global(.risk-high) { color: var(--color-danger); font-weight: 600; }
  :global(.risk-medium) { color: var(--color-warning); font-weight: 600; }
  :global(.risk-low) { color: var(--color-success); font-weight: 600; }

  .what-chart { margin-top: 0.75rem; }
  .what-chart :global(.chart-host) {
    background: var(--color-bg); border-radius: 6px; padding: 0.75rem;
  }
  .what-chart :global(.chart-canvas-wrap) { position: relative; height: 280px; }
  .what-chart :global(.chart-title) {
    font-size: 0.85rem; font-weight: 600; margin-bottom: 0.5rem;
    color: var(--color-fg-muted);
  }
  .what-error { color: var(--color-warning); font-size: 0.85rem; margin-bottom: 0.5rem; }
</style>
