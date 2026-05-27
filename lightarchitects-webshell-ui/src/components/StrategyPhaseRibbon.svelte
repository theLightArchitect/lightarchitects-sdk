<script lang="ts">
  import { authHeaders } from '$lib/auth';

  interface Props {
    /** 16-char hex nonce from the backend ResumeRegistry. */
    requestId: string;
    /** HITL question presented to the operator. */
    question: string;
    /** Short chip label (≤12 chars) shown as context header. */
    header: string;
    /** Ordered list of option labels the operator may choose. */
    options: string[];
    /** Build ID used to scope the hitl/resolve call. */
    buildId: string;
    /** Session token for confused-deputy prevention. */
    sessionId: string;
    /** Fired after a successful resolve or dismiss. */
    onResolved?: () => void;
  }

  let {
    requestId,
    question,
    header,
    options,
    buildId,
    sessionId,
    onResolved,
  }: Props = $props();

  let resolving = $state(false);
  let error = $state<string | null>(null);

  // S1-F4: consume the nonce on dismiss so it cannot be replayed until TTL.
  async function dismiss() {
    if (resolving) return;
    resolving = true;
    try {
      await fetch('/api/copilot/hitl/resolve', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ request_id: requestId, session_id: sessionId, choice: 0, dismissed: true }),
      });
    } catch { /* dismiss errors are non-fatal */ } finally {
      resolving = false;
    }
    onResolved?.();
  }

  async function resolve(choiceIndex: number) {
    if (resolving) return;
    resolving = true;
    error = null;

    try {
      const resp = await fetch('/api/copilot/hitl/resolve', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          request_id: requestId,
          session_id: sessionId,
          choice: choiceIndex,
        }),
      });

      if (resp.ok) {
        onResolved?.();
      } else {
        const body = await resp.json().catch(() => ({}));
        error = (body as { error?: string }).error ?? `HTTP ${resp.status}`;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Network error';
    } finally {
      resolving = false;
    }
  }
</script>

<!-- Strategy loop HITL pause ribbon.
     Shown when the backend StrategyDispatcher fires Outcome::Pause and the
     ResumeRegistry parks the state.  The operator selects an option which
     posts to POST /api/copilot/hitl/resolve (nonce-validated, session-bound). -->
<div
  class="ribbon"
  data-testid="strategy-phase-ribbon"
  role="alertdialog"
  aria-labelledby="ribbon-header"
  aria-describedby="ribbon-question"
>
  <div class="ribbon__top">
    <span class="ribbon__chip" id="ribbon-header">{header}</span>
    <span class="ribbon__label">STRATEGY PAUSE · HITL</span>
  </div>

  <p class="ribbon__question" id="ribbon-question">{question}</p>

  {#if error}
    <p class="ribbon__error" role="alert">{error}</p>
  {/if}

  <div class="ribbon__actions" role="group" aria-label="Strategy options">
    {#each options as option, i}
      <button
        class="ribbon__option"
        data-testid="strategy-option-{i}"
        onclick={() => resolve(i)}
        disabled={resolving}
        aria-busy={resolving}
      >
        {#if resolving}
          <span class="ribbon__spinner" aria-hidden="true"></span>
        {/if}
        {option}
      </button>
    {/each}

    <button
      class="ribbon__dismiss"
      data-testid="strategy-dismiss"
      onclick={() => void dismiss()}
      disabled={resolving}
      aria-label="Dismiss strategy HITL"
    >✕</button>
  </div>
</div>

<style>
  .ribbon {
    margin: 4px 0;
    padding: 8px 10px;
    border: 1px solid rgba(255, 165, 0, 0.35);
    border-left: 3px solid rgba(255, 165, 0, 0.8);
    background: rgba(255, 140, 0, 0.05);
    border-radius: 0 var(--la-radius-sm) var(--la-radius-sm) 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .ribbon__top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .ribbon__chip {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: rgba(255, 165, 0, 0.9);
    background: rgba(255, 165, 0, 0.12);
    border: 1px solid rgba(255, 165, 0, 0.25);
    border-radius: 3px;
    padding: 1px 5px;
  }

  .ribbon__label {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.08em;
    color: var(--la-text-dim, #888);
    text-transform: uppercase;
  }

  .ribbon__question {
    font-size: 11px;
    color: var(--la-text-bright, #f1f5f9);
    line-height: 1.45;
    margin: 0;
  }

  .ribbon__error {
    font-size: 10px;
    color: var(--la-semantic-error, #f87171);
    margin: 0;
    font-family: var(--la-font-mono, monospace);
  }

  .ribbon__actions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }

  .ribbon__option {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    padding: 3px 8px;
    border: 1px solid rgba(255, 165, 0, 0.35);
    background: rgba(255, 165, 0, 0.06);
    color: rgba(255, 165, 0, 0.9);
    border-radius: var(--la-radius-sm);
    cursor: pointer;
    transition: background 0.12s, border-color 0.12s;
  }

  .ribbon__option:hover:not(:disabled) {
    background: rgba(255, 165, 0, 0.15);
    border-color: rgba(255, 165, 0, 0.6);
  }

  .ribbon__option:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .ribbon__dismiss {
    margin-left: auto;
    font-size: 10px;
    color: var(--la-text-dim, #888);
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 3px;
    transition: color 0.1s;
  }

  .ribbon__dismiss:hover:not(:disabled) {
    color: var(--la-text-bright, #f1f5f9);
  }

  .ribbon__spinner {
    display: inline-block;
    width: 8px;
    height: 8px;
    border: 1.5px solid rgba(255, 165, 0, 0.3);
    border-top-color: rgba(255, 165, 0, 0.9);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (prefers-reduced-motion: reduce) {
    .ribbon__spinner { animation: none; }
    .ribbon__option { transition: none; }
  }
</style>
