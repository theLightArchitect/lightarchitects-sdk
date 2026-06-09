<script lang="ts">
  import { onMount } from 'svelte';
  import DOMPurify from 'dompurify';
  import { authHeaders } from '$lib/auth';
  import CodeEditor from '$lib/../components/editor/CodeEditor.svelte';
  import DiffPreview from '$lib/../components/dispatch/DiffPreview.svelte';

  interface ArtifactRow {
    name: string;
    agent: string;
    size: number;
    modified: string;
  }

  let { dispatchId }: { dispatchId: string } = $props();

  let rows = $state<ArtifactRow[]>([]);
  let loading = $state(true);
  let fetchError = $state<string | null>(null);
  let selected = $state<string | null>(null);
  let previewContent = $state('');
  let previewLoading = $state(false);
  let previewError = $state<string | null>(null);

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  }

  async function loadArtifacts(): Promise<void> {
    loading = true;
    fetchError = null;
    try {
      const res = await fetch(`/api/dispatch/${dispatchId}/artifacts`, {
        headers: authHeaders(),
      });
      if (!res.ok) {
        const body = await res.text().catch(() => '');
        throw new Error(body || `HTTP ${res.status}`);
      }
      rows = await res.json() as ArtifactRow[];
    } catch (e) {
      fetchError = e instanceof Error ? e.message : 'Failed to load artifacts';
    } finally {
      loading = false;
    }
  }

  async function openPreview(name: string): Promise<void> {
    selected = name;
    previewLoading = true;
    previewError = null;
    previewContent = '';
    try {
      const res = await fetch(
        `/api/dispatch/${dispatchId}/artifacts/${encodeURIComponent(name)}`,
        { headers: authHeaders() },
      );
      if (!res.ok) {
        const body = await res.text().catch(() => '');
        throw new Error(body || `HTTP ${res.status}`);
      }
      const raw = await res.text();
      // render_safety: html_injection_blocked + markdown_renderer_sanitization:dompurify
      // Strip all tags/attrs — content is plain text displayed in Monaco read-only view.
      previewContent = DOMPurify.sanitize(raw, { ALLOWED_TAGS: [], ALLOWED_ATTR: [] });
    } catch (e) {
      previewError = e instanceof Error ? e.message : 'Failed to load preview';
    } finally {
      previewLoading = false;
    }
  }

  async function copyToClipboard(content: string): Promise<void> {
    await navigator.clipboard.writeText(content).catch(() => {});
  }

  onMount(() => { void loadArtifacts(); });
</script>

<!-- dispatch-artifacts contract: ui_locator SQD-DISPATCH Results tab -->
<DiffPreview />
<div class="results-tab" data-testid="results-tab-panel">
  <div class="rt-header">
    <span class="rt-title">
      <span class="rt-idx">[ ARTIFACTS ]</span>
      {dispatchId}
    </span>
    <button class="rt-refresh" onclick={() => loadArtifacts()} title="Refresh artifact list">↺</button>
  </div>

  {#if loading}
    <div class="rt-state">Loading artifacts…</div>
  {:else if fetchError}
    <div class="rt-state rt-error" role="alert">
      {fetchError === 'E_ARTIFACTS_DIR_MISSING'
        ? 'No artifacts written — agent runner did not have file-write capability for this wave'
        : fetchError}
    </div>
  {:else if rows.length === 0}
    <div class="rt-state rt-empty">No artifacts yet. Run a dispatch with real agents to produce output.</div>
  {:else}
    <div class="rt-layout">
      <!-- Artifact list -->
      <ol class="rt-list" aria-label="Artifacts">
        {#each rows as row}
          <li
            class="rt-row"
            class:rt-row-selected={selected === row.name}
            onclick={() => openPreview(row.name)}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') openPreview(row.name); }}
            tabindex="0"
            role="option"
            aria-selected={selected === row.name}
          >
            <span class="rt-agent" style="color: var(--la-agent-{row.agent}, var(--la-text-dim))">
              {row.agent}
            </span>
            <span class="rt-name">{row.name}</span>
            <span class="rt-meta">{formatBytes(row.size)} · {row.modified}</span>
          </li>
        {/each}
      </ol>

      <!-- Preview pane: Monaco read-only (monaco_read_only_only: true) -->
      <div class="rt-preview">
        {#if selected === null}
          <div class="rt-state rt-empty" style="margin: auto">Select an artifact to preview</div>
        {:else if previewLoading}
          <div class="rt-state">Loading…</div>
        {:else if previewError}
          <div class="rt-state rt-error" role="alert">
            Artifact {selected} exists but cannot be read: {previewError}
          </div>
        {:else}
          <div class="rt-preview-toolbar">
            <span class="rt-preview-name">{selected}</span>
            <button class="rt-action-btn" onclick={() => copyToClipboard(previewContent)} title="Copy to clipboard">
              Copy
            </button>
          </div>
          <div class="rt-editor-wrap">
            <!-- render_safety: iframe_blocked, script_blocked enforced by CodeEditor (Monaco sandbox) -->
            <CodeEditor path={selected} content={previewContent} readonly={true} />
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .results-tab {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--la-bg-void);
    color: var(--la-text-bright);
    font-family: var(--la-font-chrome, monospace);
    overflow: hidden;
  }

  .rt-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    border-bottom: 1px solid var(--la-border, #333);
    font-size: 11px;
    flex-shrink: 0;
  }

  .rt-title { display: flex; gap: 6px; align-items: center; }
  .rt-idx { color: var(--la-text-mute, #555); font-weight: 200; }

  .rt-refresh {
    background: none;
    border: 1px solid var(--la-border, #333);
    color: var(--la-text-dim, #888);
    padding: 1px 6px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 11px;
  }
  .rt-refresh:hover { color: var(--la-text-bright); }

  .rt-state {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    font-size: 11px;
    color: var(--la-text-dim, #888);
    padding: 24px;
  }

  .rt-error { color: var(--la-agent-security, #f55); }
  .rt-empty { color: var(--la-text-mute, #555); }

  .rt-layout {
    display: grid;
    grid-template-columns: 220px 1fr;
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }

  .rt-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border-right: 1px solid var(--la-border, #333);
    overflow-y: auto;
  }

  .rt-row {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 7px 10px;
    cursor: pointer;
    border-bottom: 1px solid var(--la-border-dim, #222);
    font-size: 10px;
  }

  .rt-row:hover { background: var(--la-bg-elev-1, #111); }
  .rt-row-selected { background: var(--la-bg-elev-1, #111); outline: 1px solid var(--la-focus-ring, #FFD700); }
  .rt-row:focus { outline: 1px solid var(--la-focus-ring, #FFD700); }

  .rt-agent { font-size: 9px; text-transform: uppercase; letter-spacing: 0.05em; }
  .rt-name { color: var(--la-text-bright); font-size: 10px; word-break: break-all; }
  .rt-meta { color: var(--la-text-mute, #555); font-size: 9px; }

  .rt-preview {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 0;
  }

  .rt-preview-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px;
    border-bottom: 1px solid var(--la-border, #333);
    font-size: 10px;
    flex-shrink: 0;
  }

  .rt-preview-name { color: var(--la-text-dim, #888); }

  .rt-action-btn {
    background: none;
    border: 1px solid var(--la-border, #333);
    color: var(--la-text-dim, #888);
    padding: 1px 8px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 10px;
    font-family: inherit;
  }
  .rt-action-btn:hover { color: var(--la-text-bright); }

  .rt-editor-wrap {
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }
</style>
