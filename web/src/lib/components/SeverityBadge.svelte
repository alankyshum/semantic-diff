<script lang="ts">
  import type { Severity } from '$lib/types';

  export let severity: Severity | 'none' | string;

  // Map all six v2 severities + legacy 'none' to CSS variables. Keys are
  // lowercase — the canonical on-wire format (Rust `#[serde(rename_all="lowercase")]`).
  const tokens: Record<string, string> = {
    critical: '--color-danger',
    high: '--color-danger',
    medium: '--color-warning',
    low: '--color-info',
    nit: '--color-fg-muted',
    info: '--color-fg-muted',
    none: '--color-fg-muted',
  };

  // Defensive `.toLowerCase()`: canonical input is already lowercase, but we
  // accept any-case strings as a safety net for legacy callers.
  $: key = (severity ?? 'info').toString().toLowerCase();
  $: token = tokens[key] ?? tokens.info;
  // For 'low' fall back to --color-accent if --color-info isn't defined.
  $: cssVar = key === 'low' ? `var(--color-info, var(--color-accent))` : `var(${token})`;
  $: bg = `color-mix(in srgb, ${cssVar} 12%, transparent)`;
  $: border = `color-mix(in srgb, ${cssVar} 32%, transparent)`;
  $: isCritical = key === 'critical';
</script>

<span
  class="badge severity-{key}"
  class:critical={isCritical}
  style="background: {bg}; color: {cssVar}; border: 1px solid {border};"
>
  {severity}
</span>

<style>
  .badge {
    display: inline-block; font-size: 0.7rem; font-weight: 600; text-transform: uppercase;
    padding: 0.1rem 0.4rem; border-radius: 4px;
  }
  .badge.critical {
    font-weight: 800;
    letter-spacing: 0.04em;
  }
</style>
