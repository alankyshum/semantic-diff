<script lang="ts">
  // F20: CostChart — daily USD cost totals over the last 30 days. Pure SVG.
  import type { ResultDocument } from '$lib/types';

  export let results: ResultDocument[] = [];

  const DAYS = 30;
  const W = 600;
  const H = 180;
  const PAD_L = 40;
  const PAD_R = 12;
  const PAD_T = 12;
  const PAD_B = 28;

  interface Day {
    date: string; // YYYY-MM-DD
    cost: number;
  }

  function dayKey(iso: string): string {
    // Truncate ISO timestamp to YYYY-MM-DD (UTC). Robust to missing time portion.
    const i = iso.indexOf('T');
    return i > 0 ? iso.slice(0, i) : iso.slice(0, 10);
  }

  function addDays(d: Date, n: number): Date {
    const out = new Date(d);
    out.setUTCDate(out.getUTCDate() + n);
    return out;
  }

  function formatDay(d: Date): string {
    return d.toISOString().slice(0, 10);
  }

  function shortLabel(date: string): string {
    // e.g. "2025-04-12" → "04/12"
    const m = date.slice(5, 7);
    const d = date.slice(8, 10);
    return `${m}/${d}`;
  }

  $: days = (() => {
    const totals = new Map<string, number>();
    for (const r of results) {
      const cost = r.metadata?.tokens?.cost_usd;
      const started = r.metadata?.started_at;
      if (typeof cost !== 'number' || !started) continue;
      const key = dayKey(started);
      totals.set(key, (totals.get(key) ?? 0) + cost);
    }
    const today = new Date();
    today.setUTCHours(0, 0, 0, 0);
    const out: Day[] = [];
    for (let i = DAYS - 1; i >= 0; i--) {
      const d = addDays(today, -i);
      const key = formatDay(d);
      out.push({ date: key, cost: totals.get(key) ?? 0 });
    }
    return out;
  })();

  $: total = days.reduce((acc, d) => acc + d.cost, 0);
  $: hasData = days.some((d) => d.cost > 0);
  $: maxCost = Math.max(0.01, ...days.map((d) => d.cost));

  $: barW = (W - PAD_L - PAD_R) / DAYS;

  function fmtUsd(n: number): string {
    if (!isFinite(n)) return '$0.00';
    if (n > 0 && n < 0.01) return '<$0.01';
    return `$${n.toFixed(2)}`;
  }

  function barX(i: number): number {
    return PAD_L + i * barW + 1;
  }
  function barH(cost: number): number {
    if (cost <= 0) return 0;
    const usable = H - PAD_T - PAD_B;
    return (cost / maxCost) * usable;
  }
  function barY(cost: number): number {
    return H - PAD_B - barH(cost);
  }

  // Simple y-axis ticks: 0, mid, max.
  $: yTicks = [0, maxCost / 2, maxCost];
</script>

<div class="cost-chart">
  <div class="head">
    <h3>Cost over last 30 days</h3>
    <div class="total">30-day total: <strong>{fmtUsd(total)}</strong></div>
  </div>
  <small style="color: var(--color-fg-muted)">Completed runs only</small>

  {#if !hasData}
    <div class="empty">No cost data yet</div>
  {:else}
    <svg
      class="chart"
      viewBox="0 0 {W} {H}"
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-label="Daily review cost over last 30 days"
    >
      <!-- Y-axis grid + labels -->
      {#each yTicks as t}
        {@const y = H - PAD_B - (t / maxCost) * (H - PAD_T - PAD_B)}
        <line x1={PAD_L} x2={W - PAD_R} y1={y} y2={y} class="grid" />
        <text x={PAD_L - 6} y={y + 3} class="ylabel">{fmtUsd(t)}</text>
      {/each}
      <!-- Bars -->
      {#each days as d, i (d.date)}
        <g class="bar-g">
          <title>{d.date}: {fmtUsd(d.cost)}</title>
          <rect
            class="bar-hit"
            x={barX(i)}
            y={PAD_T}
            width={Math.max(1, barW - 2)}
            height={H - PAD_T - PAD_B}
          />
          {#if d.cost > 0}
            <rect
              class="bar"
              x={barX(i)}
              y={barY(d.cost)}
              width={Math.max(1, barW - 2)}
              height={barH(d.cost)}
            />
          {/if}
        </g>
      {/each}
      <!-- X-axis labels: every 5 days -->
      {#each days as d, i (d.date + '-x')}
        {#if i % 5 === 0 || i === days.length - 1}
          <text
            x={barX(i) + barW / 2}
            y={H - PAD_B + 14}
            class="xlabel"
          >{shortLabel(d.date)}</text>
        {/if}
      {/each}
      <!-- Axis line -->
      <line x1={PAD_L} x2={W - PAD_R} y1={H - PAD_B} y2={H - PAD_B} class="axis" />
    </svg>
  {/if}
</div>

<style>
  .cost-chart { display: flex; flex-direction: column; gap: 0.4rem; }
  .head { display: flex; justify-content: space-between; align-items: baseline; }
  .head h3 { margin: 0; font-size: 0.85rem; color: var(--color-fg-muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .total { font-size: 0.85rem; color: var(--color-fg); }
  .total strong { color: var(--color-accent); }
  .empty {
    color: var(--color-fg-muted);
    font-size: 0.85rem;
    padding: 1rem;
    text-align: center;
    border: 1px dashed var(--color-border);
    border-radius: 6px;
  }
  svg.chart { width: 100%; height: auto; max-height: 220px; }
  .bar { fill: var(--color-accent); }
  .bar-hit { fill: transparent; }
  .bar-g:hover .bar { filter: brightness(1.2); }
  .bar-g:hover .bar-hit { fill: var(--color-bg-inset); }
  .grid { stroke: var(--color-border); stroke-width: 1; stroke-dasharray: 2 3; }
  .axis { stroke: var(--color-border); stroke-width: 1; }
  .ylabel {
    fill: var(--color-fg-muted);
    font-size: 10px;
    text-anchor: end;
    font-family: monospace;
  }
  .xlabel {
    fill: var(--color-fg-muted);
    font-size: 10px;
    text-anchor: middle;
    font-family: monospace;
  }
</style>
