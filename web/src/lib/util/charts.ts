/**
 * Chart.js renderer for semantic-diff.
 * Ported from share-md skill. Supports:
 *   - ```chart fence → arbitrary Chart.js JSON config
 *   - Mermaid `pie` blocks → Chart.js doughnut
 *   - Mermaid `xychart-beta` blocks → Chart.js line/bar
 */

import {
  Chart,
  PieController, DoughnutController, BarController, LineController,
  PolarAreaController,
  ArcElement, BarElement, PointElement, LineElement,
  CategoryScale, LinearScale,
  Title, Tooltip, Legend,
  Colors,
  Filler,
} from 'chart.js';

Chart.register(
  PieController, DoughnutController, BarController, LineController,
  PolarAreaController,
  ArcElement, BarElement, PointElement, LineElement,
  CategoryScale, LinearScale,
  Title, Tooltip, Legend, Colors,
  Filler,
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

/** Render a Chart.js config JSON string into a canvas, return the wrapper element. */
export function renderJsonChart(
  jsonText: string,
  dark: boolean,
): { element: HTMLElement; error?: string } {
  applyChartTheme(dark);

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
    cfg.data.datasets.forEach((ds: any, i: number) => {
      if (!ds.backgroundColor) {
        ds.backgroundColor = ['pie', 'doughnut', 'polarArea'].includes(cfg.type)
          ? (ds.data || []).map((_: any, j: number) => PALETTE[j % PALETTE.length])
          : PALETTE[i % PALETTE.length];
      }
      if (!ds.borderColor) ds.borderColor = PALETTE[i % PALETTE.length];
    });
  }

  const wrapper = document.createElement('div');
  wrapper.className = 'chart-host';

  if (cfg.options?.plugins?.title?.text) {
    const h = document.createElement('div');
    h.className = 'chart-title';
    h.textContent = cfg.options.plugins.title.text;
    wrapper.appendChild(h);
  }

  const canvasWrap = document.createElement('div');
  canvasWrap.className = 'chart-canvas-wrap';
  canvasWrap.style.position = 'relative';
  canvasWrap.style.height = '300px';
  const canvas = document.createElement('canvas');
  canvasWrap.appendChild(canvas);
  wrapper.appendChild(canvasWrap);

  cfg.options = {
    responsive: true,
    maintainAspectRatio: false,
    ...cfg.options,
  };

  new Chart(canvas, cfg);
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
