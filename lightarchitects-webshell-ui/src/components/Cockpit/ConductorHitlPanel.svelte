<script lang="ts">
  import { authHeaders } from '$lib/auth';

  interface HitlTask {
    id: string;
    title: string;
    project: string;
    build_codename?: string;
    awaiting_assertion_id?: string;
    resolution_deadline?: string;
    priority: string;
    added?: string;
  }

  let tasks    = $state<HitlTask[]>([]);
  let resolving = $state<Record<string, boolean>>({});
  let error    = $state('');

  async function fetchTasks() {
    try {
      const res = await fetch('/api/conductor/hitl', { headers: authHeaders() });
      if (!res.ok) { error = `${res.status}`; return; }
      tasks = await res.json();
      error = '';
    } catch (e) {
      error = e instanceof Error ? e.message : 'fetch failed';
    }
  }

  $effect(() => {
    void fetchTasks();
    const interval = setInterval(() => { void fetchTasks(); }, 10_000);
    return () => clearInterval(interval);
  });

  async function resolve(id: string, action: 'approve' | 'reject') {
    resolving = { ...resolving, [id]: true };
    try {
      const res = await fetch(`/api/conductor/hitl/${id}/resolve`, {
        method: 'POST',
        headers: { ...authHeaders(), 'Content-Type': 'application/json' },
        body: JSON.stringify({ action }),
      });
      if (res.ok) {
        tasks = tasks.filter(t => t.id !== id);
      } else {
        error = `resolve ${res.status}`;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'resolve failed';
    } finally {
      const next = { ...resolving };
      delete next[id];
      resolving = next;
    }
  }

  function deadlineUrgency(deadline?: string): string {
    if (!deadline) return '';
    const mins = (new Date(deadline).getTime() - Date.now()) / 60_000;
    if (mins < 10) return 'urgent';
    if (mins < 60) return 'warn';
    return '';
  }

  function deadlineLabel(deadline?: string): string {
    if (!deadline) return '';
    const mins = Math.round((new Date(deadline).getTime() - Date.now()) / 60_000);
    if (mins < 0) return 'overdue';
    if (mins < 60) return `${mins}m`;
    return `${Math.floor(mins / 60)}h`;
  }
</script>

{#if tasks.length > 0}
  <div class="chtl-section">
    <div class="chtl-label">
      CONDUCTOR HITL
      <span class="chtl-count">{tasks.length}</span>
    </div>
    {#each tasks as t (t.id)}
      <div class="chtl-row">
        <div class="chtl-meta">
          <span class="chtl-title">{t.title}</span>
          {#if t.build_codename}
            <span class="chtl-codename">{t.build_codename}</span>
          {/if}
          {#if t.resolution_deadline}
            <span class="chtl-deadline {deadlineUrgency(t.resolution_deadline)}">
              {deadlineLabel(t.resolution_deadline)}
            </span>
          {/if}
        </div>
        <div class="chtl-actions">
          <button
            class="btn-approve"
            disabled={resolving[t.id]}
            onclick={() => resolve(t.id, 'approve')}
          >
            {resolving[t.id] ? '…' : 'APPROVE'}
          </button>
          <button
            class="btn-deny"
            disabled={resolving[t.id]}
            onclick={() => resolve(t.id, 'reject')}
          >
            {resolving[t.id] ? '…' : 'REJECT'}
          </button>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .chtl-section {
    border-top: 1px solid var(--la-hair-base);
    padding-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .chtl-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-text-mute);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .chtl-count {
    background: var(--la-semantic-warn);
    color: var(--la-bg-base);
    font-size: 7px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 2px;
  }

  .chtl-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    border: 1px solid color-mix(in srgb, var(--la-semantic-warn) 30%, transparent);
    background: color-mix(in srgb, var(--la-semantic-warn) 4%, transparent);
  }

  .chtl-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .chtl-title {
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    color: var(--la-text-base);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chtl-codename {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    color: var(--la-struct-primary);
    flex-shrink: 0;
  }

  .chtl-deadline {
    font-size: 8px;
    flex-shrink: 0;
    color: var(--la-text-mute);
  }

  .chtl-deadline.warn   { color: var(--la-semantic-warn); }
  .chtl-deadline.urgent { color: var(--la-semantic-error); font-weight: 700; }

  .chtl-actions {
    display: flex;
    gap: 6px;
  }

  .btn-approve,
  .btn-deny {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    padding: 2px 8px;
    border: 1px solid;
    cursor: pointer;
    background: transparent;
  }

  .btn-approve {
    border-color: var(--la-semantic-ok);
    color: var(--la-semantic-ok);
  }

  .btn-approve:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-semantic-ok) 12%, transparent);
  }

  .btn-deny {
    border-color: var(--la-semantic-error);
    color: var(--la-semantic-error);
  }

  .btn-deny:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-semantic-error) 12%, transparent);
  }

  .btn-approve:disabled,
  .btn-deny:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
