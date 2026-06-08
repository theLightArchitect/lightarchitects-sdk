<script lang="ts">
  import { onMount } from 'svelte';
  import FileTreeBrowser from '../components/editor/FileTreeBrowser.svelte';
  import CodeEditor from '../components/editor/CodeEditor.svelte';
  import DiffViewer from '../components/editor/DiffViewer.svelte';
  import { codeStore } from '$lib/stores';
  import { authHeaders } from '$lib/auth';
  import { page } from '$app/state';

  // Route params come from SvelteKit's /editor/[...filepath] route.
  const params = $derived(page.params);

  // Derived current file path from route params or store.
  let currentPath = $state<string | null>(null);
  let editorContent = $state('');
  let savedContent = $state('');
  let showDiff = $state(false);
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let saveMsg = $state<string | null>(null);

  // Infer cwd from config (default to '.'); the API resolves against server cwd.
  const cwd = '.';

  // Sync route params → currentPath reactively (avoids stale-closure warning).
  $effect(() => { if (params.filepath) currentPath = params.filepath; });

  $effect(() => {
    if (currentPath) void loadFile(currentPath);
  });

  async function loadFile(path: string) {
    loadError = null;
    try {
      const params = new URLSearchParams({ path });
      const res = await fetch(`/api/code/read?${params}`, { headers: authHeaders() });
      if (!res.ok) throw new Error(`${res.status}`);
      const data = await res.json() as { content: string };
      editorContent = data.content;
      savedContent = data.content;
      codeStore.set({
        path,
        content: data.content,
        savedContent: data.content,
        language: 'plaintext',
      });
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'Failed to load file';
    }
  }

  async function saveFile() {
    if (!currentPath) return;
    saving = true;
    saveMsg = null;
    try {
      const res = await fetch('/api/code/write', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ path: currentPath, content: editorContent }),
      });
      if (!res.ok) throw new Error(`${res.status}`);
      savedContent = editorContent;
      codeStore.update(b => b ? { ...b, savedContent: editorContent } : b);
      saveMsg = 'Saved';
      setTimeout(() => { saveMsg = null; }, 2000);
    } catch (e) {
      saveMsg = e instanceof Error ? e.message : 'Save failed';
    } finally {
      saving = false;
    }
  }

  function onFileOpen(path: string) {
    currentPath = path;
  }

  function onEditorChange(value: string) {
    editorContent = value;
  }

  function onDiffApplied() {
    showDiff = false;
    if (currentPath) void loadFile(currentPath);
  }

  const isDirty = $derived(editorContent !== savedContent);
</script>

<svelte:window
  onkeydown={(e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      void saveFile();
    }
  }}
/>

<div class="editor-screen" data-testid="editor-screen">
  <!-- Left panel: file tree -->
  <div class="panel-tree">
    <FileTreeBrowser {cwd} onOpen={onFileOpen} />
  </div>

  <!-- Right panel: editor or diff -->
  <div class="panel-main">
    {#if showDiff && currentPath}
      <DiffViewer
        path={currentPath}
        newContent={editorContent}
        onApply={onDiffApplied}
        onCancel={() => { showDiff = false; }}
      />
    {:else}
      <!-- Toolbar -->
      <div class="editor-toolbar">
        <span class="toolbar-path">
          {#if currentPath}
            {currentPath}{isDirty ? ' ●' : ''}
          {:else}
            — no file open —
          {/if}
        </span>
        <div class="toolbar-actions">
          {#if saveMsg}
            <span class="save-msg" class:err={saveMsg !== 'Saved'}>{saveMsg}</span>
          {/if}
          {#if currentPath}
            <button class="btn-diff" onclick={() => { showDiff = true; }}>Diff</button>
            <button class="btn-save" onclick={saveFile} disabled={saving || !isDirty}>
              {saving ? 'Saving…' : 'Save'}
            </button>
          {/if}
        </div>
      </div>

      {#if loadError}
        <div class="editor-error">{loadError}</div>
      {:else if currentPath}
        <div class="editor-body">
          <CodeEditor
            path={currentPath}
            content={editorContent}
            onChange={onEditorChange}
          />
        </div>
      {:else}
        <div class="editor-empty">
          <p>Select a file from the tree to open it.</p>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .editor-screen {
    display: flex;
    height: 100%;
    min-height: 0;
    background: var(--la-bg, #0a0a0f);
  }

  .panel-tree {
    width: 220px;
    flex-shrink: 0;
    overflow: hidden;
  }

  .panel-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .editor-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px;
    border-bottom: 1px solid var(--la-border, #1e1e2e);
    background: var(--la-surface, #0f0f1a);
    height: 34px;
    flex-shrink: 0;
  }

  .toolbar-path {
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 12px;
    color: var(--la-text-dim, #888);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .save-msg {
    font-size: 11px;
    color: var(--la-agent-researcher, #4f4);
    font-family: 'JetBrains Mono Variable', monospace;
  }

  .save-msg.err { color: var(--la-agent-security, #f55); }

  .btn-save, .btn-diff {
    padding: 2px 10px;
    border-radius: 3px;
    border: 1px solid var(--la-border, #333);
    background: none;
    cursor: pointer;
    font-size: 11px;
    font-family: 'JetBrains Mono Variable', monospace;
    color: var(--la-text, #ccc);
  }

  .btn-save:not(:disabled):hover { border-color: var(--la-agent-researcher, #4f4); color: var(--la-agent-researcher, #4f4); }
  .btn-save:disabled { opacity: 0.35; cursor: default; }
  .btn-diff:hover { border-color: var(--la-text, #ccc); }

  .editor-body {
    flex: 1;
    min-height: 0;
  }

  .editor-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-dim, #555);
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 13px;
  }

  .editor-error {
    padding: 16px;
    color: var(--la-agent-security, #f55);
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 12px;
  }
</style>
