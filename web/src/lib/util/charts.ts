/**
 * Chart.js renderer for semantic-diff.
 *
 * Aligned with the `share--markdown` skill's renderer (see
 * dotfiles/.../skills/share--markdown/spa/src/lib/charts.ts):
 *
 *   - Mermaid `pie` blocks         → Chart.js doughnut
 *   - Mermaid `xychart-beta` blocks → Chart.js line/bar
 *       - With `%% @chart-ext v1` header → enhanced renderer
 *         (fills, annotations, dual-axis, per-series styling).
 *   - ```chart fenced JSON         → arbitrary Chart.js config.
 *
 * The Mermaid component in semantic-diff intercepts pie/xychart blocks
 * before mermaid renders them, so the user sees a real interactive
 * Chart.js canvas instead of a static SVG.
 */

import {
  Chart,
  PieController, DoughnutController, BarController, LineController,
  PolarAreaController,
  ArcElement, BarElement, PointElement, LineElement,
  CategoryScale, LinearScale, TimeScale,
  Title, Tooltip, Legend,
  Colors,
  Filler,
} from 'chart.js';
import annotationPlugin from 'chartjs-plugin-annotation';
import yaml from 'js-yaml';

Chart.register(
  PieController, DoughnutController, BarController, LineController,
  PolarAreaController,
  ArcElement, BarElement, PointElement, LineElement,
  CategoryScale, LinearScale, TimeScale,
  Title, Tooltip, Legend, Colors,
  Filler,
  annotationPlugin,
);

const PALETTE = [
  '#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6',
  '#06b6d4', '#ec4899', '#84cc16', '#f97316', '#6366f1',
  '#14b8a6', '#a855f7', '#eab308', '#0ea5e9', '#d946ef',
];

export function applyChartTheme(dark: boolean) {
  Chart.defaults.color = dark ? '#e6edf3' : '#1f2328';
  Chart.defaults.borderColor = dark ? '#30363d' : '#d0d7de';
  Chart.defaults.font.family = 'system-ui, -apple-system, sans-serif';
}

interface ChartContext { dark: boolean }

function makeWrapper(title?: string): { wrapper: HTMLElement; canvas: HTMLCanvasElement } {
  const wrapper = document.createElement('div');
  wrapper.className = 'chart-host';
  if (title) {
    const h = document.createElement('div');
    h.className = 'chart-title';
    h.textContent = title;
    wrapper.appendChild(h);
  }
  const canvasWrap = document.createElement('div');
  canvasWrap.className = 'chart-canvas-wrap';
  canvasWrap.style.position = 'relative';
  canvasWrap.style.height = '320px';
  const canvas = document.createElement('canvas');
  canvasWrap.appendChild(canvas);
  wrapper.appendChild(canvasWrap);
  return { wrapper, canvas };
}

// ─── Mermaid pie parser ─────────────────────────────────────────────────────
// Syntax:
//   pie [showData] title <text>
//     "Label A" : 30
//     "Label B" : 70
export function parseMermaidPie(source: string): { title?: string; labels: string[]; values: number[]; showData: boolean } | null {
  const lines = source.split(/\r?\n/);
  let title: string | undefined;
  let showData = false;
  const labels: string[] = [];
  const values: number[] = [];
  let sawPie = false;

  for (const raw of lines) {
    const line = raw.replace(/%%.*$/, '').trim();
    if (!line) continue;
    if (/^pie\b/i.test(line)) {
      sawPie = true;
      const rest = line.replace(/^pie\b/i, '').trim();
      if (/showData/i.test(rest)) showData = true;
      const tm = rest.match(/title\s+(.+)$/i);
      if (tm) title = tm[1].trim();
      continue;
    }
    if (/^title\s+/i.test(line)) {
      title = line.replace(/^title\s+/i, '').trim();
      continue;
    }
    if (/^showData/i.test(line)) { showData = true; continue; }
    const m = line.match(/^"([^"]*)"\s*:\s*([\d.]+)\s*$/);
    if (m) {
      labels.push(m[1]);
      values.push(parseFloat(m[2]));
    }
  }
  if (!sawPie || labels.length === 0) return null;
  return { title, labels, values, showData };
}

// ─── Mermaid xychart parser ─────────────────────────────────────────────────
// Syntax (simplified subset):
//   xychart-beta [horizontal]
//     title "Sales"
//     x-axis [Jan, Feb, Mar, …]   OR   x-axis "label" 0 --> 100
//     y-axis "label" [min --> max]
//     bar [v1, v2, v3, …]
//     line [v1, v2, v3, …]
export function parseMermaidXyChart(source: string): {
  title?: string;
  horizontal: boolean;
  xCategories: string[] | null;
  xLabel?: string;
  yLabel?: string;
  series: { type: 'bar' | 'line'; data: number[] }[];
} | null {
  const lines = source.split(/\r?\n/);
  let sawHeader = false;
  let title: string | undefined;
  let horizontal = false;
  let xCategories: string[] | null = null;
  let xLabel: string | undefined;
  let yLabel: string | undefined;
  const series: { type: 'bar' | 'line'; data: number[] }[] = [];

  for (const raw of lines) {
    const line = raw.replace(/%%.*$/, '').trim();
    if (!line) continue;
    if (/^xychart-beta\b/i.test(line)) {
      sawHeader = true;
      if (/horizontal/i.test(line)) horizontal = true;
      continue;
    }
    if (/^title\s+/i.test(line)) {
      title = line.replace(/^title\s+/i, '').trim().replace(/^"(.*)"$/, '$1');
      continue;
    }
    const xCat = line.match(/^x-axis\s+\[([^\]]+)\]/i);
    if (xCat) {
      xCategories = xCat[1].split(',').map(s => s.trim().replace(/^"(.*)"$/, '$1'));
      continue;
    }
    const xLab = line.match(/^x-axis\s+"([^"]+)"/i);
    if (xLab) { xLabel = xLab[1]; continue; }
    const yLab = line.match(/^y-axis\s+"([^"]+)"/i);
    if (yLab) { yLabel = yLab[1]; continue; }
    const bar = line.match(/^bar\s+\[([^\]]+)\]/i);
    if (bar) {
      series.push({ type: 'bar', data: bar[1].split(',').map(v => parseFloat(v.trim())) });
      continue;
    }
    const ln = line.match(/^line\s+\[([^\]]+)\]/i);
    if (ln) {
      series.push({ type: 'line', data: ln[1].split(',').map(v => parseFloat(v.trim())) });
      continue;
    }
  }
  if (!sawHeader || series.length === 0) return null;
  if (!xCategories) {
    const n = Math.max(...series.map(s => s.data.length));
    xCategories = Array.from({ length: n }, (_, i) => String(i + 1));
  }
  return { title, horizontal, xCategories, xLabel, yLabel, series };
}

// ─── Chart Extension Types ──────────────────────────────────────────────────
interface ChartExtension {
  series?: Array<{
    label?: string;
    color?: string;
    width?: number;
    dash?: number[];
    points?: boolean;
    fill_to?: number | 'next' | 'prev';
    y_axis?: 'left' | 'right';
  }>;
  fills?: Array<{
    above?: number;
    below?: number;
    between?: [number, number];
    color: string;
    label?: string;
  }>;
  annotations?: Array<
    | { type: 'hline'; value: number; color?: string; dash?: number[]; label?: string }
    | { type: 'vline'; value: number | string; color?: string; dash?: number[]; label?: string }
    | { type: 'box'; x_range: [unknown, unknown]; y_range: [number, number]; color?: string; label?: string }
  >;
  legend?: { position?: 'top' | 'bottom' | 'left' | 'right' | 'none'; display?: boolean };
  y_axis_right?: { label?: string; range?: [number, number] };
  interaction?: { tooltip?: 'index' | 'nearest' | 'point'; hover_animations?: boolean };
}

// Detect `%% @chart-ext v1` marker and parse subsequent `%% <yaml>` lines.
export function parseChartExtension(source: string): ChartExtension | null {
  const lines = source.split(/\r?\n/);
  const markerIdx = lines.findIndex(l => /^%%\s+@chart-ext\s+v1\s*$/.test(l));
  if (markerIdx === -1) return null;

  const yamlLines: string[] = [];
  for (let i = markerIdx + 1; i < lines.length; i++) {
    const line = lines[i];
    if (/^\s*xychart-beta\b/i.test(line)) break;
    if (/^\s*%%\{.*\}%%\s*$/.test(line)) continue;
    if (line.trim() === '') continue;
    const m = line.match(/^%%\s?(.*)$/);
    if (!m) break;
    const content = m[1];
    if (/^@chart-ext\s+v1\s*$/.test(content)) continue;
    yamlLines.push(content);
  }

  if (yamlLines.length === 0) return null;

  try {
    const parsed = yaml.load(yamlLines.join('\n')) as ChartExtension;
    return (parsed && typeof parsed === 'object') ? parsed : null;
  } catch (e) {
    console.warn('[chart-ext] YAML parse error:', e);
    return null;
  }
}

// ─── Helpers ────────────────────────────────────────────────────────────────
function withAlpha(color: string, alpha: number): string {
  if (color.startsWith('#')) {
    const hex = color.slice(1);
    const expanded = hex.length === 3
      ? hex.split('').map(c => c + c).join('')
      : hex;
    const bigint = parseInt(expanded, 16);
    const r = (bigint >> 16) & 255;
    const g = (bigint >> 8) & 255;
    const b = bigint & 255;
    return `rgba(${r},${g},${b},${alpha})`;
  }
  return color;
}

// ─── Renderers ──────────────────────────────────────────────────────────────
export function renderPieChart(
  parsed: NonNullable<ReturnType<typeof parseMermaidPie>>,
  ctx: ChartContext,
): HTMLElement {
  applyChartTheme(ctx.dark);
  const { wrapper, canvas } = makeWrapper(parsed.title);
  const total = parsed.values.reduce((a, b) => a + b, 0);
  // Defer construction until canvas is mounted.
  queueMicrotask(() => {
    new Chart(canvas, {
      type: 'doughnut',
      data: {
        labels: parsed.labels,
        datasets: [{
          data: parsed.values,
          backgroundColor: parsed.values.map((_, i) => PALETTE[i % PALETTE.length]),
          borderWidth: 1,
          borderColor: ctx.dark ? '#0d1117' : '#ffffff',
        }],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        cutout: '55%',
        plugins: {
          legend: { position: 'right' },
          tooltip: {
            callbacks: {
              label(item) {
                const v = item.parsed as number;
                const pct = total > 0 ? ((v / total) * 100).toFixed(1) : '0';
                return `${item.label}: ${v.toLocaleString()} (${pct}%)`;
              },
            },
          },
        },
      },
    });
  });
  return wrapper;
}

export function renderXyChart(
  parsed: NonNullable<ReturnType<typeof parseMermaidXyChart>>,
  ctx: ChartContext,
): HTMLElement {
  applyChartTheme(ctx.dark);
  const { wrapper, canvas } = makeWrapper(parsed.title);
  const datasets = parsed.series.map((s, i) => ({
    type: s.type as 'bar' | 'line',
    label: `Series ${i + 1}`,
    data: s.data,
    backgroundColor: s.type === 'bar' ? PALETTE[i % PALETTE.length] : 'transparent',
    borderColor: PALETTE[i % PALETTE.length],
    borderWidth: 2,
    tension: 0.3,
    fill: false,
    pointRadius: s.type === 'line' ? 4 : 0,
    pointBackgroundColor: PALETTE[i % PALETTE.length],
  }));

  const primary = parsed.series[0].type;
  queueMicrotask(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    new Chart(canvas, {
      type: primary,
      data: { labels: parsed.xCategories!, datasets: datasets as any },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        indexAxis: parsed.horizontal ? 'y' : 'x',
        plugins: {
          legend: { display: parsed.series.length > 1, position: 'top' },
          tooltip: { mode: 'index', intersect: false },
        },
        scales: {
          x: { title: { display: !!parsed.xLabel, text: parsed.xLabel } },
          y: { title: { display: !!parsed.yLabel, text: parsed.yLabel } },
        },
        interaction: { mode: 'nearest', axis: 'x', intersect: false },
      },
    } as any);
  });
  return wrapper;
}

export function renderEnhancedXyChart(
  baseParsed: NonNullable<ReturnType<typeof parseMermaidXyChart>>,
  ext: ChartExtension,
  ctx: ChartContext,
): HTMLElement {
  applyChartTheme(ctx.dark);
  const { wrapper, canvas } = makeWrapper(baseParsed.title);
  const xLabels = baseParsed.xCategories!;

  const datasets = baseParsed.series.map((s, i) => {
    const extSeries = ext.series?.[i] ?? {};
    const color = extSeries.color ?? PALETTE[i % PALETTE.length];
    const isLine = s.type === 'line';

    return {
      type: s.type as 'bar' | 'line',
      label: extSeries.label ?? `Series ${i + 1}`,
      data: s.data,
      borderColor: color,
      backgroundColor: isLine
        ? (extSeries.fill_to !== undefined ? withAlpha(color, 0.2) : 'transparent')
        : color,
      borderWidth: extSeries.width ?? (isLine ? 2 : 0),
      borderDash: extSeries.dash,
      pointRadius: isLine ? (extSeries.points ? 4 : 0) : 0,
      pointHoverRadius: isLine ? (extSeries.points ? 7 : 0) : 0,
      pointBackgroundColor: color,
      tension: 0.3,
      fill: extSeries.fill_to !== undefined ? extSeries.fill_to : false,
      yAxisID: extSeries.y_axis === 'right' ? 'y1' : 'y',
    };
  });

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const annotations: Record<string, any> = {};
  let annoCounter = 0;
  (ext.fills ?? []).forEach(fill => {
    const id = `fill_${annoCounter++}`;
    if (fill.above !== undefined) {
      annotations[id] = {
        type: 'box', yMin: fill.above, yMax: undefined,
        backgroundColor: fill.color, borderWidth: 0,
        label: fill.label
          ? { content: fill.label, display: true, position: { x: 'start', y: 'start' }, color: '#666' }
          : undefined,
      };
    } else if (fill.below !== undefined) {
      annotations[id] = {
        type: 'box', yMin: undefined, yMax: fill.below,
        backgroundColor: fill.color, borderWidth: 0,
        label: fill.label
          ? { content: fill.label, display: true, position: { x: 'start', y: 'end' }, color: '#666' }
          : undefined,
      };
    } else if (fill.between) {
      const [a, b] = fill.between;
      annotations[id] = {
        type: 'box', yMin: Math.min(a, b), yMax: Math.max(a, b),
        backgroundColor: fill.color, borderWidth: 0,
        label: fill.label ? { content: fill.label, display: true, color: '#666' } : undefined,
      };
    }
  });
  (ext.annotations ?? []).forEach(anno => {
    const id = `anno_${annoCounter++}`;
    if (anno.type === 'hline') {
      annotations[id] = {
        type: 'line', yMin: anno.value, yMax: anno.value,
        borderColor: anno.color ?? '#9ca3af', borderWidth: 1.5, borderDash: anno.dash,
        label: anno.label
          ? { content: anno.label, display: true, position: 'end', color: anno.color ?? '#666' }
          : undefined,
      };
    } else if (anno.type === 'vline') {
      let xVal: unknown = anno.value;
      const idx = xLabels.findIndex(l => String(l) === String(xVal));
      if (idx !== -1) xVal = idx;
      annotations[id] = {
        type: 'line', xMin: xVal, xMax: xVal,
        borderColor: anno.color ?? '#9ca3af', borderWidth: 1.5, borderDash: anno.dash,
        label: anno.label
          ? { content: anno.label, display: true, position: 'start', color: anno.color ?? '#666' }
          : undefined,
      };
    } else if (anno.type === 'box') {
      annotations[id] = {
        type: 'box',
        xMin: anno.x_range[0], xMax: anno.x_range[1],
        yMin: anno.y_range[0], yMax: anno.y_range[1],
        backgroundColor: anno.color ?? 'rgba(200,200,200,0.2)', borderWidth: 0,
        label: anno.label ? { content: anno.label, display: true, color: '#666' } : undefined,
      };
    }
  });

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const scales: Record<string, any> = {
    x: { title: { display: !!baseParsed.xLabel, text: baseParsed.xLabel } },
    y: { title: { display: !!baseParsed.yLabel, text: baseParsed.yLabel }, position: 'left' },
  };
  if (ext.y_axis_right) {
    scales.y1 = {
      title: { display: !!ext.y_axis_right.label, text: ext.y_axis_right.label },
      position: 'right',
      grid: { drawOnChartArea: false },
      min: ext.y_axis_right.range?.[0],
      max: ext.y_axis_right.range?.[1],
    };
  }

  const primary = baseParsed.series[0].type;
  const showLegend = ext.legend?.display ?? (datasets.length > 1);
  const legendPos = (ext.legend?.position === 'none' || !ext.legend?.position)
    ? 'bottom'
    : ext.legend.position as 'top' | 'bottom' | 'left' | 'right';

  queueMicrotask(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    new Chart(canvas, {
      type: primary,
      data: { labels: xLabels, datasets: datasets as any },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        indexAxis: baseParsed.horizontal ? 'y' : 'x',
        plugins: {
          legend: {
            display: showLegend && ext.legend?.position !== 'none',
            position: legendPos,
          },
          tooltip: { mode: ext.interaction?.tooltip ?? 'index', intersect: false },
          annotation: { annotations },
        },
        scales,
        interaction: { mode: 'nearest', axis: 'x', intersect: false },
      },
    } as any);
  });
  return wrapper;
}

/** Render a Chart.js config JSON string into a canvas, return the wrapper element. */
export function renderJsonChart(
  jsonText: string,
  dark: boolean,
): { element: HTMLElement; error?: string } {
  applyChartTheme(dark);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let cfg: any;
  try {
    cfg = JSON.parse(jsonText);
  } catch (e) {
    return { element: document.createElement('div'), error: `Chart JSON parse error: ${(e as Error).message}` };
  }
  if (!cfg || typeof cfg !== 'object' || !cfg.type) {
    return { element: document.createElement('div'), error: 'Chart JSON missing required "type" field' };
  }

  // Auto-color datasets
  if (cfg.data && Array.isArray(cfg.data.datasets)) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    cfg.data.datasets.forEach((ds: any, i: number) => {
      if (!ds.backgroundColor) {
        ds.backgroundColor = ['pie', 'doughnut', 'polarArea'].includes(cfg.type)
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          ? (ds.data || []).map((_: any, j: number) => PALETTE[j % PALETTE.length])
          : PALETTE[i % PALETTE.length];
      }
      if (!ds.borderColor) ds.borderColor = PALETTE[i % PALETTE.length];
    });
  }

  const { wrapper, canvas } = makeWrapper(cfg.options?.plugins?.title?.text);

  cfg.options = {
    responsive: true,
    maintainAspectRatio: false,
    ...cfg.options,
  };

  queueMicrotask(() => {
    new Chart(canvas, cfg);
  });
  return { element: wrapper };
}

/** Extract ```chart blocks from content. Returns array of {json, startIndex, endIndex}. */
export function extractChartBlocks(raw: string): Array<{ json: string; start: number; end: number }> {
  const re = /```chart\n([\s\S]*?)```/g;
  const out: Array<{ json: string; start: number; end: number }> = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(raw)) !== null) {
    out.push({ json: m[1].trim(), start: m.index, end: m.index + m[0].length });
  }
  return out;
}

/** True when the mermaid block source is a `pie` chart. */
export function isMermaidPie(source: string): boolean {
  return /^\s*(?:%%[^\n]*\n\s*)*pie\b/i.test(source);
}

/** True when the mermaid block source is an xychart. */
export function isMermaidXyChart(source: string): boolean {
  return /xychart-beta\b/i.test(source);
}
