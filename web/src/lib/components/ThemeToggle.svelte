<script lang="ts">
  import { themePref, cycleTheme } from '$lib/stores/theme';

  $: icon = $themePref === 'light' ? '☀️' : $themePref === 'dark' ? '🌙' : '🖥️';
  $: label = $themePref === 'auto'
    ? 'Theme: auto (follows system). Click to switch.'
    : `Theme: ${$themePref}. Click to switch.`;
</script>

<button type="button" class="theme-toggle" aria-label={label} title={label} on:click={cycleTheme}>
  <span aria-hidden="true">{icon}</span>
</button>

<style>
  .theme-toggle {
    background: transparent;
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    border-radius: 6px;
    width: 32px;
    height: 32px;
    line-height: 1;
    font-size: 1rem;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background 0.1s, border-color 0.1s;
  }
  .theme-toggle:hover {
    background: var(--color-bg-inset);
    border-color: var(--color-accent);
  }
  .theme-toggle:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
</style>
