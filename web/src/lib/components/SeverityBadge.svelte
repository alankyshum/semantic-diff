<script lang="ts">
  export let severity: 'high' | 'medium' | 'low' | 'none' | string;

  // Map severities to CSS variables.
  const tokens: Record<string, string> = {
    high: '--color-danger',
    medium: '--color-warning',
    low: '--color-success',
    none: '--color-fg-muted',
  };

  $: token = tokens[severity?.toLowerCase()] ?? tokens.none;
  // Use color-mix where supported so the badge has tinted bg/border without
  // needing extra tokens. Falls back gracefully (var() resolves; color-mix
  // is supported in all modern targets we care about).
  $: bg = `color-mix(in srgb, var(${token}) 12%, transparent)`;
  $: border = `color-mix(in srgb, var(${token}) 32%, transparent)`;
  $: color = `var(${token})`;
</script>

<span class="badge" style="background: {bg}; color: {color}; border: 1px solid {border};">
  {severity}
</span>

<style>
  .badge {
    display: inline-block; font-size: 0.7rem; font-weight: 600; text-transform: uppercase;
    padding: 0.1rem 0.4rem; border-radius: 4px;
  }
</style>
