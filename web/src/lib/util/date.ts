export function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

export function formatDuration(ms: number | null | undefined): string {
  if (ms == null) return '–';
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s <= 60) return `${s.toFixed(1)} s`;
  const m = Math.floor(s / 60);
  const rem = Math.round(s - m * 60);
  return `${m}m ${rem}s`;
}

export function statusColor(status: string): string {
  switch (status) {
    case 'complete': return 'var(--color-success)';
    case 'running': return 'var(--color-warning)';
    case 'failed': return 'var(--color-danger)';
    default: return 'var(--color-fg-muted)';
  }
}
