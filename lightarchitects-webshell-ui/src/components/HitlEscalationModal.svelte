<!--
@component
Blocking operator modal for autonomous-build HITL escalations.

Listens for `la:escalation` window events (dispatched by sse.ts) filtered to
`buildId`. Pending items from `GET /api/builds/:id/autonomous/status` can be
passed via `initialItems` so the modal recovers correctly after a page reload.

Props:
- `buildId`      — UUID of the active build; events for other builds are ignored
- `initialItems` — pending HITL items from the status poll (merged on change)
-->
<script lang="ts">
  import { authHeaders } from '$lib/auth';
  import type { EscalationEvent } from '$lib/types';

  interface HitlItem {
    call_id:      string;
    build_id:     string;
    task_id?:     string;
    wave_index:   number;
    worker_slot?: number;
    reason:       string;
    created_at?:  string;
  }

  let {
    buildId,
    initialItems = [],
  }: {
    buildId:       string;
    initialItems?: HitlItem[];
  } = $props();

  let queue      = $state<HitlItem[]>([]);
  let resolving  = $state(false);
  let reasonText = $state('');
  let error      = $state('');

  let current = $derived(queue[0] ?? null);

  // ── Merge initialItems whenever the prop changes ───────────────────────────

  $effect(() => {
    if (initialItems.length === 0) return;
    const newItems = initialItems.filter(it => !queue.some(q => q.call_id === it.call_id));
    if (newItems.length > 0) {
      queue = [...queue, ...newItems];
    }
  });

  // ── Live escalation events from sse.ts ────────────────────────────────────

  function onEscalation(e: Event) {
    const ev = (e as CustomEvent).detail as EscalationEvent;
    if (ev.build_id !== buildId) return;
    if (queue.some(q => q.call_id === ev.call_id)) return;
    queue = [...queue, {
      call_id:     ev.call_id,
      build_id:    ev.build_id,
      wave_index:  ev.wave_index,
      worker_slot: ev.worker_slot,
      reason:      ev.reason,
      created_at:  new Date().toISOString(),
    }];
  }

  $effect(() => {
    window.addEventListener('la:escalation', onEscalation);
    return () => window.removeEventListener('la:escalation', onEscalation);
  });

  // ── Resolve (approve / reject) ────────────────────────────────────────────

  async function resolve(approved: boolean) {
    if (!current || resolving) return;
    const item = current;
    resolving = true;
    error = '';
    try {
      const res = await fetch(
        `/api/builds/${item.build_id}/hitl/${item.call_id}`,
        {
          method:  'POST',
          headers: { ...authHeaders(), 'Content-Type': 'application/json' },
          body:    JSON.stringify({ approved, reason: reasonText || null }),
        },
      );
      if (res.ok) {
        queue = queue.slice(1);
        reasonText = '';
      } else {
        error = `${res.status} — ${await res.text()}`;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'request failed';
    } finally {
      resolving = false;
    }
  }
</script>

{#if current}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="hitl-backdrop"
    role="presentation"
    onkeydown={(e) => { if (e.key === 'Escape') e.preventDefault(); }}
  >
    <div
      class="hitl-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="hitl-title"
    >
      <div class="hitl-badge">HITL ESCALATION</div>

      <h2 id="hitl-title" class="hitl-title">Operator Decision Required</h2>

      <div class="hitl-meta">
        <div class="hitl-row-meta">
          <span class="hitl-key">WAVE</span>
          <span class="hitl-val">{current.wave_index}</span>
        </div>
        {#if current.worker_slot !== undefined}
          <div class="hitl-row-meta">
            <span class="hitl-key">SLOT</span>
            <span class="hitl-val">{current.worker_slot}</span>
          </div>
        {/if}
        {#if current.task_id}
          <div class="hitl-row-meta">
            <span class="hitl-key">TASK</span>
            <span class="hitl-val hitl-mono">{current.task_id}</span>
          </div>
        {/if}
        <div class="hitl-row-meta">
          <span class="hitl-key">ID</span>
          <span class="hitl-val hitl-mono">{current.call_id.slice(0, 8)}…</span>
        </div>
      </div>

      <div class="hitl-reason">
        <span class="hitl-reason-label">REASON</span>
        <p class="hitl-reason-text">{current.reason}</p>
      </div>

      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="hitl-label-field">Operator note (optional)</label>
      <textarea
        id="hitl-reason-input"
        class="hitl-textarea"
        rows="2"
        placeholder="Add context for the audit log…"
        bind:value={reasonText}
        disabled={resolving}
        aria-label="Operator note"
      ></textarea>

      {#if error}
        <p class="hitl-error" role="alert">{error}</p>
      {/if}

      {#if queue.length > 1}
        <p class="hitl-queue-hint">
          {queue.length - 1} more escalation{queue.length > 2 ? 's' : ''} queued
        </p>
      {/if}

      <div class="hitl-actions">
        <button
          class="btn-reject"
          disabled={resolving}
          onclick={() => resolve(false)}
        >
          {resolving ? '…' : 'REJECT'}
        </button>
        <button
          class="btn-approve"
          disabled={resolving}
          onclick={() => resolve(true)}
        >
          {resolving ? '…' : 'APPROVE'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .hitl-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(4px);
  }

  .hitl-modal {
    background: var(--la-bg-panel);
    border: 1px solid var(--la-semantic-warn);
    border-radius: 6px;
    padding: 24px;
    width: min(480px, 92vw);
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow: 0 0 48px color-mix(in srgb, var(--la-semantic-warn) 20%, transparent);
  }

  .hitl-badge {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-bg-base);
    background: var(--la-semantic-warn);
    padding: 2px 8px;
    border-radius: 2px;
    width: fit-content;
  }

  .hitl-title {
    font-size: 15px;
    font-weight: 700;
    color: var(--la-text-bright);
    margin: 0;
  }

  .hitl-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .hitl-row-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .hitl-key {
    font-size: 8px;
    letter-spacing: 0.1em;
    color: var(--la-text-dim);
    font-weight: 600;
  }

  .hitl-val {
    font-size: 12px;
    font-weight: 700;
    color: var(--la-text-bright);
  }

  .hitl-mono {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    font-weight: 400;
  }

  .hitl-reason {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--la-semantic-warn) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--la-semantic-warn) 25%, transparent);
    border-radius: 4px;
  }

  .hitl-reason-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-semantic-warn);
  }

  .hitl-reason-text {
    font-size: 11px;
    color: var(--la-text-base);
    line-height: 1.5;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .hitl-label-field {
    font-size: 9px;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    font-weight: 600;
  }

  .hitl-textarea {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    background: var(--la-bg-elev-1);
    border: 1px solid var(--la-hair-base);
    border-radius: 3px;
    color: var(--la-text-base);
    padding: 6px 8px;
    resize: vertical;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }

  .hitl-textarea:focus {
    border-color: var(--la-focus-ring);
  }

  .hitl-textarea:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .hitl-error {
    font-size: 10px;
    color: var(--la-semantic-error);
    margin: 0;
  }

  .hitl-queue-hint {
    font-size: 9px;
    color: var(--la-text-dim);
    margin: 0;
    font-style: italic;
  }

  .hitl-actions {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }

  .btn-approve,
  .btn-reject {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 6px 20px;
    border: 1px solid;
    border-radius: 3px;
    cursor: pointer;
    background: transparent;
    transition: background 150ms;
  }

  .btn-approve {
    border-color: var(--la-semantic-ok);
    color: var(--la-semantic-ok);
  }

  .btn-approve:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-semantic-ok) 14%, transparent);
  }

  .btn-reject {
    border-color: var(--la-semantic-error);
    color: var(--la-semantic-error);
  }

  .btn-reject:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-semantic-error) 14%, transparent);
  }

  .btn-approve:disabled,
  .btn-reject:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
