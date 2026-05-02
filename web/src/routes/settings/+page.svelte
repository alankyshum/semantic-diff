<script lang="ts">
  import { onMount } from 'svelte';
  import SettingsForm from '$lib/components/SettingsForm.svelte';
  import type { ConfigPayload, ProbeReport, RawConfig } from '$lib/types';
  import { defaultRawConfig } from '$lib/types';

  let config: ConfigPayload = { path: null, exists: false, raw: defaultRawConfig() };
  let probe: ProbeReport = { providers: [] };
  let loadingConfig = true;
  let loadingProbe = true;
  let configError = '';
  let probeError = '';
  let csrfToken = '';

  let saving = false;
  let saveError = '';
  let saveBanner: '' | 'saved' = '';
  let saveBannerTimer: ReturnType<typeof setTimeout> | null = null;

  async function loadCsrfToken() {
    try {
      const res = await fetch('/api/csrf-token');
      if (!res.ok) return;
      const body = (await res.json()) as { token?: string };
      if (body && typeof body.token === 'string') csrfToken = body.token;
    } catch {
      // Non-fatal: PUT will surface 403 if token is missing.
    }
  }

  async function loadConfig() {
    loadingConfig = true;
    configError = '';
    try {
      const res = await fetch('/api/config');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data: ConfigPayload = await res.json();
      // Normalize so SettingsForm always sees a fully-populated shape.
      const merged: RawConfig = { ...defaultRawConfig(), ...(data.raw ?? {}) };
      merged.claude = { ...defaultRawConfig().claude, ...(data.raw?.claude ?? {}) };
      merged.copilot = { ...defaultRawConfig().copilot, ...(data.raw?.copilot ?? {}) };
      merged.cursor = { ...defaultRawConfig().cursor, ...(data.raw?.cursor ?? {}) };
      config = { path: data.path, exists: data.exists, raw: merged, parse_error: data.parse_error };
    } catch (e) {
      configError = String(e);
    } finally {
      loadingConfig = false;
    }
  }

  async function loadProbe() {
    loadingProbe = true;
    probeError = '';
    try {
      const res = await fetch('/api/config/probe');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      probe = (await res.json()) as ProbeReport;
    } catch (e) {
      probeError = String(e);
    } finally {
      loadingProbe = false;
    }
  }

  function refreshProbe() {
    loadProbe();
  }

  async function handleSave(e: CustomEvent<RawConfig>) {
    saveError = '';
    saving = true;
    if (saveBannerTimer) {
      clearTimeout(saveBannerTimer);
      saveBannerTimer = null;
    }
    saveBanner = '';
    try {
      const res = await fetch('/api/config', {
        method: 'PUT',
        headers: {
          'content-type': 'application/json',
          'X-CSRF-Token': csrfToken,
        },
        body: JSON.stringify(e.detail),
      });
      if (res.status === 200) {
        // Update from server's canonical response if it returns one; otherwise keep what we sent.
        const body = await res.json().catch(() => null);
        if (body && typeof body === 'object') {
          // Backend returns RawConfig (per spec); accept either RawConfig directly or full envelope.
          if ('raw' in body) {
            config = body as ConfigPayload;
          } else {
            config = { ...config, exists: true, raw: body as RawConfig };
          }
        } else {
          config = { ...config, exists: true, raw: e.detail };
        }
        saveBanner = 'saved';
        saveBannerTimer = setTimeout(() => {
          saveBanner = '';
        }, 3000);
      } else if (res.status === 403) {
        const body = await res
          .json()
          .catch(() => ({ error: `HTTP ${res.status}` }) as { error: string });
        saveError = (body && body.error) || `HTTP ${res.status}`;
      } else if (res.status === 422) {
        const body = await res
          .json()
          .catch(() => ({ error: `HTTP ${res.status}` }) as { error: string });
        saveError = (body && body.error) || `HTTP ${res.status}`;
      } else {
        saveError = `Unexpected response: HTTP ${res.status}`;
      }
    } catch (err) {
      saveError = String(err);
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    loadCsrfToken();
    loadConfig();
    loadProbe();
  });
</script>

<svelte:head>
  <title>Settings — semantic-diff</title>
</svelte:head>

<div class="container">
  <a href="/" class="back-link">← Back</a>
  <h1>Settings</h1>
  <p class="path-hint">
    Editing: <code>{config.path ?? '(no home dir found)'}</code>
    {#if !config.exists}<span class="muted">(file does not exist yet)</span>{/if}
  </p>

  {#if saveBanner === 'saved'}
    <div class="banner banner-ok" role="status">Saved.</div>
  {/if}
  {#if saveError}
    <div class="banner banner-err" role="alert">{saveError}</div>
  {/if}
  {#if config.parse_error}
    <div class="banner banner-warn" role="alert">
      ⚠ Existing config could not be parsed: {config.parse_error}. Saving will overwrite it.
    </div>
  {/if}

  {#if loadingConfig}
    <div class="muted loading">Loading config…</div>
  {:else if configError}
    <div class="banner banner-err" role="alert">Failed to load config: {configError}</div>
  {:else}
    <SettingsForm initial={config.raw} {saving} on:save={handleSave} />
  {/if}

  <section class="provider-status" aria-labelledby="probe-heading">
    <h2 id="probe-heading">Detected providers</h2>
    {#if loadingProbe}
      <p class="muted">Probing providers…</p>
    {:else if probeError}
      <div class="banner banner-err" role="alert">Failed to probe providers: {probeError}</div>
    {:else}
      {#if probe.providers.length === 0}
        <p class="muted">No providers reported.</p>
      {:else}
        {#each probe.providers as p (p.name)}
          <article class="provider">
            <h3 class="provider-name">{p.name}</h3>
            {#each p.binaries as b (b.name)}
              <div class="bin-row">
                <code class="bin-name">{b.name}</code>
                {#if b.found}
                  <span class="badge ok">found</span>
                  {#if b.path}<code class="path">{b.path}</code>{/if}
                  {#if b.version_status === 'timeout'}
                    <span class="badge timeout">timed out</span>
                  {:else if b.version_status === 'error'}
                    <span class="badge err">version error</span>
                  {:else if b.version}
                    <code class="version">{b.version}</code>
                  {/if}
                {:else}
                  <span class="badge missing">not found</span>
                {/if}
              </div>
            {/each}
          </article>
        {/each}
      {/if}
      <button type="button" class="btn btn-small refresh" on:click={refreshProbe}>
        Re-detect
      </button>
    {/if}
  </section>
</div>

<style>
  .container {
    max-width: 720px;
    margin: 0 auto;
    padding: 1.5rem clamp(1rem, 4vw, 2rem) 3rem;
  }
  .back-link {
    display: inline-block;
    color: var(--color-fg-muted);
    font-size: 0.85rem;
    margin-bottom: 0.75rem;
  }
  .back-link:hover {
    color: var(--color-accent);
  }
  h1 {
    font-size: 1.6rem;
    margin: 0 0 0.5rem;
  }
  .path-hint {
    color: var(--color-fg-muted);
    font-size: 0.85rem;
    margin: 0 0 1.25rem;
    word-break: break-all;
  }
  .path-hint code {
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0.05rem 0.45rem;
    font-size: 0.78rem;
    color: var(--color-fg);
  }
  .muted {
    color: var(--color-fg-muted);
    font-size: 0.85rem;
  }
  .loading {
    padding: 1rem 0;
  }

  .banner {
    border-radius: 6px;
    padding: 0.55rem 0.85rem;
    margin: 0 0 1rem;
    font-size: 0.85rem;
  }
  .banner-ok {
    border: 1px solid var(--color-success);
    background: color-mix(in srgb, var(--color-success) 10%, transparent);
    color: var(--color-fg);
  }
  .banner-err {
    border: 1px solid var(--color-danger);
    background: color-mix(in srgb, var(--color-danger) 10%, transparent);
    color: var(--color-fg);
  }
  .banner-warn {
    border: 1px solid var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 12%, transparent);
    color: var(--color-fg);
  }

  .provider-status {
    margin-top: 2rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--color-border);
  }
  .provider-status h2 {
    margin: 0 0 0.75rem;
    font-size: 1rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-fg-muted);
  }
  .provider {
    border: 1px solid var(--color-border);
    background: var(--color-bg-elev);
    border-radius: 6px;
    padding: 0.7rem 0.9rem;
    margin-bottom: 0.6rem;
  }
  .provider-name {
    margin: 0 0 0.5rem;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--color-fg);
    text-transform: capitalize;
  }
  .bin-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    font-size: 0.8rem;
    margin-bottom: 0.25rem;
  }
  .bin-row:last-child {
    margin-bottom: 0;
  }
  .bin-name {
    font-family: 'Fira Code', 'Cascadia Code', monospace;
    color: var(--color-fg);
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0.05rem 0.35rem;
    font-size: 0.75rem;
  }
  .badge {
    font-size: 0.7rem;
    padding: 0.05rem 0.5rem;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 600;
  }
  .badge.ok {
    background: color-mix(in srgb, var(--color-success) 18%, transparent);
    color: var(--color-success);
    border: 1px solid var(--color-success);
  }
  .badge.missing {
    background: color-mix(in srgb, var(--color-danger) 18%, transparent);
    color: var(--color-danger);
    border: 1px solid var(--color-danger);
  }
  .badge.timeout {
    background: color-mix(in srgb, var(--color-warning) 18%, transparent);
    color: var(--color-warning);
    border: 1px solid var(--color-warning);
  }
  .badge.err {
    background: color-mix(in srgb, var(--color-danger) 18%, transparent);
    color: var(--color-danger);
    border: 1px solid var(--color-danger);
  }
  .path,
  .version {
    font-family: 'Fira Code', 'Cascadia Code', monospace;
    font-size: 0.72rem;
    color: var(--color-fg-muted);
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0.05rem 0.35rem;
    overflow-wrap: anywhere;
  }

  .btn {
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
    color: var(--color-fg);
    border-radius: 6px;
    padding: 0.45rem 0.9rem;
    font-size: 0.85rem;
    cursor: pointer;
    font-family: inherit;
  }
  .btn:hover:not(:disabled) {
    background: var(--color-bg-elev);
    border-color: var(--color-accent);
  }
  .btn-small {
    padding: 0.3rem 0.7rem;
    font-size: 0.78rem;
  }
  .refresh {
    margin-top: 0.5rem;
  }
</style>
