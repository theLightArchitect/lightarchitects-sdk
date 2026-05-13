<script lang="ts">
  import { authHeaders } from '$lib/auth';

  interface Props {
    path: string;
    newContent: string;
    onApply?: () => void;
    onCancel?: () => void;
  }

  let { path, newContent, onApply, onCancel }: Props = $props();

  interface DiffLine {
    tag: '+' | '-' | ' ';
    text: string;
  }

  let diffLines = $state<DiffLine[]>([]);
  let hasChanges = $state(false);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let applying = $state(false);

  $effect(() => {
    void loadDiff(path, newContent);
  });

  async function loadDiff(p: string, content: string) {
    loading = true;
    error = null;
    try {
      const res = await fetch('/api/code/preview-diff', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ path: p, new_content: content }),
      });
      if (!res.ok) throw new Error(`${res.status}`);
      const data = await res.json() as { diff: string; has_changes: boolean };
      hasChanges = data.has_changes;
      diffLines = parseDiff(data.diff);
    } catch (e) {
      error = e instanceof Error ? e.message : 'failed to compute diff';
    } finally {
      loading = false;
    }
  }

  function parseDiff(diff: string): DiffLine[] {
    return diff
      .split('\n')
      .filter(l => !l.startsWith('---') && !l.startsWith('+++') && !l.startsWith('@@'))
      .map(l => {
        if (l.startsWith('+')) return { tag: '+' as const, text: l.slice(1) };
        if (l.startsWith('-')) return { tag: '-' as const, text: l.slice(1) };
        return { tag: ' ' as const, text: l.startsWith(' ') ? l.slice(1) : l };
      });
  }

  async function applyDiff() {
    applying = true;
    try {
      const res = await fetch('/api/code/apply-diff', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ path, diff: buildDiff() }),
      });
      if (!res.ok) throw new Error(`${res.status}`);
      onApply?.();
    } catch (e) {
      error = e instanceof Error ? e.message : 'apply failed';
    } finally {
      applying = false;
    }
  }

  function buildDiff(): string {
    return diffLines
      .map(l => `${l.tag}${l.text}`)
      .join('\n');
  }
</script>

<div class="diff-viewer" data-testid="diff-viewer">
  <div class="diff-header">
    <span class="diff-title">DIFF — {path}</span>
    <div class="diff-actions">
      {#if hasChanges}
        <button class="btn-apply" onclick={applyDiff} disabled={applying}>
          {applying ? 'Applying…' : 'Apply'}
        </button>
      {/if}
      <button class="btn-cancel" onclick={onCancel}>Cancel</button>
    </div>
  </div>

  {#if loading}
    <div class="diff-state">Computing diff…</div>
  {:else if error}
    <div class="diff-state error">{error}</div>
  {:else if !hasChanges}
    <div class="diff-state">No changes</div>
  {:else}
    <div class="diff-body">
      {#each diffLines as line, i (i)}
        <div
          class="diff-line"
          class:added={line.tag === '+'}
          class:removed={line.tag === '-'}
        >
          <span class="diff-gutter">{line.tag}</span>
          <span class="diff-text">{line.text}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .diff-viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--la-bg, #0a0a0f);
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 12px;
  }

  .diff-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-bottom: 1px solid var(--la-border, #1e1e2e);
    background: var(--la-surface, #0f0f1a);
  }

  .diff-title {
    color: var(--la-text-dim, #888);
    font-size: 11px;
    letter-spacing: 0.05em;
  }

  .diff-actions { display: flex; gap: 6px; }

  .btn-apply, .btn-cancel {
    padding: 2px 10px;
    border-radius: 3px;
    border: 1px solid var(--la-border, #333);
    background: none;
    cursor: pointer;
    font-size: 11px;
    font-family: inherit;
  }

  .btn-apply { color: var(--la-agent-researcher, #4f4); border-color: var(--la-agent-researcher, #4f4); }
  .btn-apply:disabled { opacity: 0.4; cursor: default; }
  .btn-cancel { color: var(--la-text-dim, #888); }

  .diff-state {
    padding: 16px;
    color: var(--la-text-dim, #666);
    text-align: center;
  }
  .diff-state.error { color: var(--la-agent-security, #f55); }

  .diff-body {
    overflow: auto;
    flex: 1;
  }

  .diff-line {
    display: flex;
    gap: 6px;
    padding: 0 8px;
    white-space: pre;
    line-height: 1.5;
  }

  .diff-line.added   { background: rgba(0, 255, 0, 0.05); color: #6f6; }
  .diff-line.removed { background: rgba(255, 0, 0, 0.05); color: #f66; }

  .diff-gutter {
    width: 12px;
    flex-shrink: 0;
    user-select: none;
    color: inherit;
    opacity: 0.6;
  }

  .diff-text { flex: 1; }
</style>
