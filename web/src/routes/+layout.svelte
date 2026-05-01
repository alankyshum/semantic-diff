<script lang="ts">
  import type { LayoutData } from './$types';
</script>

<svelte:head>
  <title>semantic-diff</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, monospace;
      background: #0d1117;
      color: #e6edf3;
      min-height: 100vh;
    }
    a { color: #58a6ff; text-decoration: none; }
    a:hover { text-decoration: underline; }
    code, pre { font-family: 'Fira Code', 'Cascadia Code', monospace; }
  </style>
</svelte:head>

<slot />
