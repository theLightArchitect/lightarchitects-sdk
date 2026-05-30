<!--
@component
Blocking operator modal for ironclaw HITL escalations.

Listens for `la:ironclaw_hitl_escalation` window events AND the
`ironclawHitlEscalation` store. Resolution POSTs to:

  POST /api/control { command: "ironclaw_hitl_resolution", escalation_nonce, approved, operator_reason }

The nonce is NEVER displayed in the UI or included in audit-visible log fields
(CWE-209 — carried only in the request body). After resolution the store is
cleared; `la:ironclaw_hitl_resolution` arrival will also clear it automatically.
-->
<script lang="ts">
  import { ironclawHitlEscalation } from '$lib/stores';
  import { authHeaders } from '$lib/auth';

  let pending   = $derived($ironclawHitlEscalation);
  let resolving = $state(false);
  let reason    = $state('');
  let error     = $state('');

  async function resolve(approved: boolean) {
    if (!pending || resolving) return;
    const nonce = pending.nonce;
    resolving = true;
    error = '';
    try {
      const res = await fetch('/api/control', {
        method:  'POST',
        headers: { ...authHeaders(), 'Content-Type': 'application/json' },
        body: JSON.stringify({
          command:           'ironclaw_hitl_resolution',
          escalation_nonce:  nonce,
          approved,
          operator_reason:   reason || null,
        }),
      });
      if (res.ok || res.status === 204) {
        ironclawHitlEscalation.set(null);
        reason = '';
      } else {
        error = `${res.status} — ${await res.text().catch(() => 'unknown')}`;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'request failed';
    } finally {
      resolving = false;
    }
  }

  function deadlineLabel(iso?: string): string {
    if (!iso) return '';
    const mins = Math.round((new Date(iso).getTime() - Date.now()) / 60_000);
    if (mins < 0) return 'overdue';
    if (mins < 60) return `${mins}m`;
    return `${Math.floor(mins / 60)}h`;
  }

  function deadlineClass(iso?: string): string {
    if (!iso) return '';
    const mins = (new Date(iso).getTime() - Date.now()) / 60_000;
    if (mins < 10) return 'deadline-urgent';
    if (mins < 60) return 'deadline-warn';
    return '';
  }
</script>

{#if pending}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="ic-backdrop"
    role="presentation"
    onkeydown={(e) => { if (e.key === 'Escape') e.preventDefault(); }}
  >
    <div
      class="ic-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ic-hitl-title"
    >
      <div class="ic-badge">IRONCLAW HITL</div>

      <h2 id="ic-hitl-title" class="ic-title">Operator Decision Required</h2>

      <div class="ic-meta">
        <div class="ic-field">
          <span class="ic-key">TASK</span>
          <span class="ic-val ic-mono">{pending.task_id}</span>
        </div>
        <div class="ic-field">
          <span class="ic-key">TOPIC</span>
          <span class="ic-val">{pending.decision_topic}</span>
        </div>
        <div class="ic-field">
          <span class="ic-key">LAYER</span>
          <span class="ic-val">{pending.layer_failed}</span>
        </div>
        {#if pending.deadline}
          <div class="ic-field">
            <span class="ic-key">DEADLINE</span>
            <span class="ic-val {deadlineClass(pending.deadline)}">{deadlineLabel(pending.deadline)}</span>
          </div>
        {/if}
      </div>

      <div class="ic-question">
        <span class="ic-question-label">QUESTION</span>
        <p class="ic-question-text">{pending.escalation_question}</p>
      </div>

      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="ic-label-field">Operator note (optional)</label>
      <textarea
        id="ic-reason"
        class="ic-textarea"
        rows="2"
        placeholder="Add context for the helix decision record…"
        bind:value={reason}
        disabled={resolving}
        aria-label="Operator note"
      ></textarea>

      {#if error}
        <p class="ic-error" role="alert">{error}</p>
      {/if}

      <div class="ic-actions">
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
  .ic-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.75);
    backdrop-filter: blur(4px);
  }

  .ic-modal {
    background: var(--la-bg-panel);
    border: 1px solid var(--la-danger-stroke, var(--la-semantic-warn));
    border-radius: 6px;
    padding: 24px;
    width: min(500px, 94vw);
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow: 0 0 56px color-mix(in srgb, var(--la-danger-stroke, var(--la-semantic-warn)) 18%, transparent);
  }

  .ic-badge {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-bg-base);
    background: var(--la-danger-stroke, var(--la-semantic-warn));
    padding: 2px 8px;
    border-radius: 2px;
    width: fit-content;
  }

  .ic-title {
    font-size: 15px;
    font-weight: 700;
    color: var(--la-text-bright);
    margin: 0;
  }

  .ic-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .ic-field {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .ic-key {
    font-size: 8px;
    letter-spacing: 0.1em;
    color: var(--la-text-dim);
    font-weight: 600;
  }

  .ic-val {
    font-size: 12px;
    font-weight: 700;
    color: var(--la-text-bright);
  }

  .ic-mono {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    font-weight: 400;
  }

  .ic-val.deadline-warn   { color: var(--la-semantic-warn); }
  .ic-val.deadline-urgent { color: var(--la-semantic-error); }

  .ic-question {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--la-danger-stroke, var(--la-semantic-warn)) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--la-danger-stroke, var(--la-semantic-warn)) 28%, transparent);
    border-radius: 4px;
  }

  .ic-question-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-danger-stroke, var(--la-semantic-warn));
  }

  .ic-question-text {
    font-size: 12px;
    color: var(--la-text-base);
    line-height: 1.55;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .ic-label-field {
    font-size: 9px;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    font-weight: 600;
  }

  .ic-textarea {
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

  .ic-textarea:focus {
    border-color: var(--la-focus-ring);
  }

  .ic-textarea:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .ic-error {
    font-size: 10px;
    color: var(--la-semantic-error);
    margin: 0;
  }

  .ic-actions {
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
