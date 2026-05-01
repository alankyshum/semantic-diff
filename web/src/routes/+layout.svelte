<script lang="ts">
  import '$lib/styles/tokens.css';
  import { onMount } from 'svelte';
  import { effectiveTheme, applyTheme } from '$lib/stores/theme';
  import type { LayoutData } from './$types';

  onMount(() => {
    const unsub = effectiveTheme.subscribe((t) => applyTheme(t));
    return () => unsub();
  });
</script>

<svelte:head>
  <title>semantic-diff</title>
</svelte:head>

<slot />
