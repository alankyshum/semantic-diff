import type { ResultDocument, ResultSummary } from './types';

const API_BASE = '/api';

export async function fetchResults(): Promise<ResultSummary[]> {
  const res = await fetch(`${API_BASE}/results`);
  if (!res.ok) throw new Error(`Failed to fetch results: ${res.status}`);
  return res.json();
}

export async function fetchResultsForRepo(repoName: string): Promise<ResultSummary[]> {
  const res = await fetch(`${API_BASE}/repos/${encodeURIComponent(repoName)}/results`);
  if (res.status === 404) return [];
  if (!res.ok) throw new Error(`Failed to fetch results for repo: ${res.status}`);
  return res.json();
}

export async function fetchResult(id: string): Promise<ResultDocument> {
  const res = await fetch(`${API_BASE}/result/${id}`);
  if (!res.ok) throw new Error(`Result not found: ${id}`);
  return res.json();
}

/**
 * Subscribe to SSE updates for a result.
 * Calls onUpdate whenever a section is updated.
 * Returns a cleanup function.
 */
export function subscribeToResult(
  id: string,
  onUpdate: (groupId: string) => void,
  onComplete: () => void,
): () => void {
  const es = new EventSource(`${API_BASE}/result/${id}/events`);

  es.addEventListener('section-updated', (e) => {
    const groupId = e.data as string;
    if (groupId === 'complete') {
      onComplete();
      es.close();
    } else {
      onUpdate(groupId);
    }
  });

  es.onerror = () => {
    // Auto-reconnect is handled by the browser, but we close on completion
  };

  return () => es.close();
}
