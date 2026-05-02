import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import CostChart from './CostChart.svelte';
import type { ResultDocument } from '$lib/types';

function mkResult(id: string, started_at: string, cost: number | undefined): ResultDocument {
  return {
    schema_version: 1,
    id,
    title: id,
    created_at: started_at,
    source: { kind: 'git_args', value: '' },
    diff: { raw: '', files: [], binary_files: [] },
    groups: [],
    reviews: {},
    status: 'complete',
    metadata: {
      tool_version: 'test',
      schema_version: 1,
      started_at,
      cli_argv: [],
      working_dir: '/',
      timings: [],
      skill_files: [],
      tokens: cost === undefined ? undefined : { cost_usd: cost },
    },
  };
}

function todayIso(offsetDays: number): string {
  const d = new Date();
  d.setUTCHours(12, 0, 0, 0);
  d.setUTCDate(d.getUTCDate() - offsetDays);
  return d.toISOString();
}

describe('CostChart', () => {
  it('shows empty state when no cost data', () => {
    const { getByText } = render(CostChart, {
      props: {
        results: [
          mkResult('a', todayIso(2), undefined),
          mkResult('b', todayIso(5), undefined),
        ],
      },
    });
    expect(getByText('No cost data yet')).toBeTruthy();
  });

  it('aggregates cost by day and computes 30-day total', () => {
    const results = [
      mkResult('a', todayIso(0), 1.5),
      mkResult('b', todayIso(0), 2.5), // same day → 4.0
      mkResult('c', todayIso(3), 1.0),
      mkResult('old', todayIso(60), 99.0), // outside 30-day window — excluded
    ];
    const { getByText, container } = render(CostChart, { props: { results } });
    // 4.0 + 1.0 = 5.0
    expect(getByText('$5.00')).toBeTruthy();
    // Bars rendered for non-zero days. There should be exactly 2 visible bars
    // (cost > 0). Hit rectangles cover all 30 days.
    const bars = container.querySelectorAll('rect.bar');
    expect(bars.length).toBe(2);
  });

  it('ignores results outside 30-day window', () => {
    const { getByText } = render(CostChart, {
      props: { results: [mkResult('old', todayIso(45), 5.0)] },
    });
    expect(getByText('No cost data yet')).toBeTruthy();
  });

  it('handles results with no metadata gracefully', () => {
    const r: ResultDocument = {
      schema_version: 1,
      id: 'x',
      title: 'x',
      created_at: todayIso(0),
      source: { kind: 'git_args', value: '' },
      diff: { raw: '', files: [], binary_files: [] },
      groups: [],
      reviews: {},
      status: 'complete',
    };
    const { getByText } = render(CostChart, { props: { results: [r] } });
    expect(getByText('No cost data yet')).toBeTruthy();
  });
});
