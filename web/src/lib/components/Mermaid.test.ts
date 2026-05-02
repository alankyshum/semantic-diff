import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import Mermaid from './Mermaid.svelte';

const renderMock = vi.fn(async (id: string, _body: string) => ({ svg: `<svg id="${id}"></svg>` }));
const initializeMock = vi.fn();

vi.mock('mermaid', () => ({
  default: {
    initialize: (...args: unknown[]) => initializeMock(...args),
    render: (id: string, body: string) => renderMock(id, body),
  },
}));

describe('Mermaid', () => {
  beforeEach(() => {
    renderMock.mockClear();
    initializeMock.mockClear();
  });

  it('renders a single fenced block', async () => {
    const content = '```mermaid\nflowchart LR\nA-->B\n```';
    const { container } = render(Mermaid, { props: { content } });
    await waitFor(() => expect(renderMock).toHaveBeenCalled());
    expect(renderMock).toHaveBeenCalledTimes(1);
    expect(renderMock.mock.calls[0][1]).toContain('flowchart LR');
    expect(renderMock.mock.calls[0][1]).toContain('A-->B');
    await waitFor(() => {
      expect(container.querySelectorAll('figure.mermaid-container').length).toBe(1);
    });
  });

  it('renders multiple fenced blocks', async () => {
    const content =
      '```mermaid\nflowchart LR\nA-->B\n```\n\nsome text\n\n' +
      '```mermaid\nflowchart TD\nC-->D\n```\n\n' +
      '```mermaid\ngraph LR\nE-->F\n```';
    const { container } = render(Mermaid, { props: { content } });
    await waitFor(() => expect(renderMock).toHaveBeenCalledTimes(3));
    await waitFor(() => {
      expect(container.querySelectorAll('figure.mermaid-container').length).toBe(3);
    });
  });

  it('extracts %% caption from first line of block', async () => {
    const content = '```mermaid\n%% control flow\nflowchart LR\nA-->B\n```';
    const { container } = render(Mermaid, { props: { content } });
    await waitFor(() => expect(renderMock).toHaveBeenCalled());
    await waitFor(() => {
      const cap = container.querySelector('figcaption');
      expect(cap).toBeTruthy();
      expect(cap?.textContent).toBe('control flow');
    });
  });

  it('falls back to raw content when no fence present', async () => {
    const content = 'flowchart LR\nA-->B';
    render(Mermaid, { props: { content } });
    await waitFor(() => expect(renderMock).toHaveBeenCalledTimes(1));
    expect(renderMock.mock.calls[0][1]).toContain('flowchart LR');
  });
});
