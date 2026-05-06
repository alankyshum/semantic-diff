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

  function parse(raw: string) {
    // Try to extract JSON array from content (LLM may include prose before/after)
    const jsonMatch = raw.match(/\[[\s\S]*?\]/);
    const chartMatch = raw.match(/```chart\n([\s\S]*?)```/);

    if (jsonMatch) {
      try {
        const parsed = JSON.parse(jsonMatch[0]);
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

    if (chartMatch) {
      chartJson = chartMatch[1].trim();
    }

    // Fallback: if no JSON found, the LLM may have returned a markdown table
    if (rows.length === 0 && !parseError) {
      parseError = '';  // Let MarkdownView handle it
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
