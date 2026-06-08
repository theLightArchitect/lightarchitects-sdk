<script lang="ts">
  import { authHeaders } from '$lib/auth';

  interface Props {
    cwd: string;
    onOpen: (path: string) => void;
  }

  let { cwd, onOpen }: Props = $props();

  interface DirEntry { name: string; is_dir: boolean; size: number; }

  let path = $state('.');
  let entries = $state<DirEntry[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  // breadcrumb stack: array of relative path segments
  let stack = $state<string[]>([]);

  async function loadDir(rel: string) {
    loading = true;
    error = null;
    try {
      const params = new URLSearchParams({ path: rel || '.' });
      const res = await fetch(`/api/code/list?${params}`, { headers: authHeaders() });
      if (!res.ok) throw new Error(`${res.status}`);
      const data = await res.json() as { entries: DirEntry[] };
      entries = data.entries;
      path = rel || '.';
    } catch (e) {
      error = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  $effect(() => { loadDir('.'); });

  function enterDir(name: string) {
    const next = path === '.' ? name : `${path}/${name}`;
    stack = [...stack, name];
    loadDir(next);
  }

  function goBack() {
    const newStack = stack.slice(0, -1);
    stack = newStack;
    const parent = newStack.join('/') || '.';
    loadDir(parent);
  }

  function handleOpen(entry: DirEntry) {
    if (entry.is_dir) {
      enterDir(entry.name);
    } else {
      const filePath = path === '.' ? entry.name : `${path}/${entry.name}`;
      onOpen(filePath);
    }
  }

  function ext(name: string): string {
    const i = name.lastIndexOf('.');
    return i >= 0 ? name.slice(i + 1).toLowerCase() : '';
  }

  function icon(entry: DirEntry): string {
    if (entry.is_dir) return '📁';
    const e = ext(entry.name);
    if (['rs'].includes(e)) return '🦀';
    if (['ts', 'tsx'].includes(e)) return '🔷';
    if (['svelte'].includes(e)) return '🟠';
    if (['json', 'toml', 'yaml', 'yml'].includes(e)) return '📋';
    if (['md'].includes(e)) return '📝';
    return '📄';
  }
</script>

<div class="file-tree" data-testid="file-tree-browser">
  <div class="tree-header">
    {#if stack.length > 0}
      <button class="back-btn" onclick={goBack} aria-label="Go up">↩</button>
    {/if}
    <span class="tree-path">{path === '.' ? cwd : path}</span>
    <button class="refresh-btn" onclick={() => loadDir(path)} aria-label="Refresh">↻</button>
  </div>

  {#if loading}
    <div class="tree-state">loading…</div>
  {:else if error}
    <div class="tree-state error">{error}</div>
  {:else if entries.length === 0}
    <div class="tree-state">— empty —</div>
  {:else}
    <ul class="tree-list">
      {#each entries as entry (entry.name)}
        <li>
          <button
            class="entry-row"
            class:dir={entry.is_dir}
            onclick={() => handleOpen(entry)}
            title={entry.name}
          >
            <span class="entry-icon">{icon(entry)}</span>
            <span class="entry-name">{entry.name}</span>
            {#if !entry.is_dir}
              <span class="entry-size">{(entry.size / 1024).toFixed(1)}k</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .file-tree {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--la-bg, #0a0a0f);
    border-right: 1px solid var(--la-border, #1e1e2e);
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 12px;
  }

  .tree-header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--la-border, #1e1e2e);
    background: var(--la-surface, #0f0f1a);
    min-height: 32px;
  }

  .tree-path {
    flex: 1;
    color: var(--la-text-dim, #666);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .back-btn, .refresh-btn {
    background: none;
    border: none;
    color: var(--la-text-dim, #888);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 2px;
    font-size: 14px;
    line-height: 1;
  }

  .back-btn:hover, .refresh-btn:hover {
    color: var(--la-text, #ccc);
    background: var(--la-hover, rgba(255,255,255,0.05));
  }

  .tree-state {
    padding: 12px 8px;
    color: var(--la-text-dim, #666);
    text-align: center;
  }

  .tree-state.error { color: var(--la-agent-security, #f55); }

  .tree-list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow-y: auto;
    flex: 1;
  }

  .entry-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 3px 8px;
    background: none;
    border: none;
    color: var(--la-text, #ccc);
    cursor: pointer;
    text-align: left;
    font-size: 12px;
    font-family: inherit;
  }

  .entry-row:hover {
    background: var(--la-hover, rgba(255,255,255,0.05));
  }

  .entry-row.dir { color: var(--la-text, #e0e0ff); }

  .entry-icon { width: 16px; text-align: center; flex-shrink: 0; }

  .entry-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry-size {
    color: var(--la-text-dim, #555);
    font-size: 10px;
    flex-shrink: 0;
  }
</style>
