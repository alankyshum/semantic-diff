<script lang="ts">
  export let severity: 'high' | 'medium' | 'low' | 'none' | string;

  const colors: Record<string, string> = {
    high: '#f85149',
    medium: '#d29922',
    low: '#3fb950',
    none: '#8b949e',
  };

  $: color = colors[severity?.toLowerCase()] ?? colors.none;
</script>

<span class="badge" style="background: {color}20; color: {color}; border: 1px solid {color}50;">
  {severity}
</span>

<style>
  .badge {
    display: inline-block; font-size: 0.7rem; font-weight: 600; text-transform: uppercase;
    padding: 0.1rem 0.4rem; border-radius: 4px;
  }
</style>
