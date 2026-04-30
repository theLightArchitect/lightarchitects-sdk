<script lang="ts">
  /**
   * DiffPreview — operator-gated FS mutation modal (#47).
   *
   * Listens for `la:fs-mutation-pending` custom events (dispatched from
   * sse.ts when the backend broadcasts an FsMutationPending). Renders the
   * unified diff with a syntax-coloured presentation and Approve / Reject
   * actions that call the dispatch API.
   *
   * Backend wiring (mantis-rebase pending):
   * - Agent invokes Edit/Write → backend gateway intercepts → computes diff
   *   → broadcasts FsMutationPending → holds the tool-call response
   * - On approve: POST /api/dispatch/:id/fs-approve releases the tool call
   * - On reject: POST /api/dispatch/:id/fs-reject returns synthetic error
   *
   * Until backend lands, see `lib/diff-preview.ts::triggerMockDiffPreview()`
   * for local dev verification.
   */
  import {
    type FsMutationPending,
    type FsMutationPendingEvent,
    approveMutation,
    rejectMutation,
  } from '$lib/diff-preview';
  import { DOMAIN_AGENT_COLORS } from '$lib/design-tokens';

  let pending = $state<FsMutationPending | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    function onMutationPending(e: Event) {
      const detail = (e as CustomEvent<FsMutationPendingEvent>).detail;
      // Only show the first pending — queueing multiple mutations could land
      // in a follow-up. For now, an in-flight prompt blocks new ones.
      if (!pending) {
        pending = detail;
        error = null;
      }
    }
    window.addEventListener('la:fs-mutation-pending', onMutationPending);
    return () => {
      window.removeEventListener('la:fs-mutation-pending', onMutationPending);
    };
  });

  // Tokenise the diff into rows so we can colour add/del lines without
  // pulling in a dependency. Leading char on each row drives the class.
  let diffRows = $derived.by(() => {
    if (!pending) return [];
    return pending.diff_unified.split('\n').map((line, idx) => ({
      idx,
      kind:
        line.startsWith('+++') || line.startsWith('---')
          ? 'header'
          : line.startsWith('@@')
            ? 'hunk'
            : line.startsWith('+')
              ? 'add'
              : line.startsWith('-')
                ? 'del'
                : 'ctx',
      text: line,
    }));
  });

  let agentColor = $derived(
    pending && (DOMAIN_AGENT_COLORS as Record<string, string>)[pending.agent]
      ? (DOMAIN_AGENT_COLORS as Record<string, string>)[pending.agent]
      : '#FFD700',
  );

  async function approve() {
    if (!pending || busy) return;
    busy = true;
    error = null;
    try {
      await approveMutation(pending.dispatch_id, pending.mutation_id);
      pending = null;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function reject(reason: string) {
    if (!pending || busy) return;
    busy = true;
    error = null;
    try {
      await rejectMutation(pending.dispatch_id, pending.mutation_id, reason);
      pending = null;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (!pending || busy) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      reject('operator-dismissed');
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if pending}
  <div
    class="diff-scrim"
    role="dialog"
    aria-modal="true"
    aria-labelledby="diff-title"
    data-testid="diff-preview"
  >
    <div class="diff-modal">
      <header class="diff-header">
        <div class="diff-title-row">
          <span
            class="diff-agent-badge"
            style="background-color: {agentColor}; box-shadow: 0 0 6px {agentColor};"
            aria-label="agent {pending.agent}"
          ></span>
          <strong id="diff-title">{pending.agent}</strong>
          <span class="diff-tool">{pending.tool}</span>
          <code class="diff-path">{pending.file_path}</code>
        </div>
        <p class="diff-explainer">
          The agent wants to write this change. Review and Approve to commit, or Reject to abort.
          <kbd>Esc</kbd> rejects.
        </p>
      </header>

      <div class="diff-body">
        {#each diffRows as row (row.idx)}
          <div class="diff-row" data-kind={row.kind}>
            <span class="diff-line-num">{row.idx + 1}</span>
            <span class="diff-line-text">{row.text}</span>
          </div>
        {/each}
      </div>

      {#if error}
        <p class="diff-error" role="alert">{error}</p>
      {/if}

      <footer class="diff-actions">
        <button
          class="diff-btn la-destructive"
          onclick={() => reject('operator-rejected')}
          disabled={busy}
        >
          Reject
        </button>
        <button class="diff-btn primary" onclick={approve} disabled={busy}>
          {busy ? 'Working…' : 'Approve & commit'}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .diff-scrim {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: rgba(10, 10, 15, 0.78);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    animation: diff-fade-in var(--la-transition-fast) ease-out;
  }
  .diff-modal {
    width: min(880px, 92vw);
    max-height: 84vh;
    display: flex;
    flex-direction: column;
    background: #0d1117;
    border: 1px solid #1e293b;
    border-radius: var(--la-radius-lg);
    box-shadow:
      0 12px 32px rgba(0, 0, 0, 0.5),
      0 0 0 1px rgba(255, 215, 0, 0.08);
    color: var(--la-text-body);
    font-family: var(--la-font-chrome);
  }
  .diff-header {
    padding: 14px 18px 8px;
    border-bottom: 1px solid #1e293b;
  }
  .diff-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .diff-agent-badge {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .diff-tool {
    color: var(--la-text-mute);
    font-family: var(--la-font-mono);
    font-size: 11px;
    padding: 1px 6px;
    background: #1e293b;
    border-radius: var(--la-radius-sm);
  }
  .diff-path {
    flex: 1;
    color: var(--la-text-body);
    font-family: var(--la-font-mono);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .diff-explainer {
    margin: 6px 0 0;
    color: var(--la-text-mute);
    font-size: 11px;
  }
  .diff-explainer kbd {
    font-family: var(--la-font-mono);
    background: #1e293b;
    padding: 1px 5px;
    border-radius: var(--la-radius-sm);
    font-size: 10px;
  }
  .diff-body {
    flex: 1;
    overflow: auto;
    padding: 8px 0;
    background: #0a0a0f;
    font-family: var(--la-font-mono);
    font-size: 12px;
    line-height: 1.45;
  }
  .diff-row {
    display: grid;
    grid-template-columns: 50px 1fr;
    padding: 0 8px;
  }
  .diff-row[data-kind="header"] { color: var(--la-text-mute); }
  .diff-row[data-kind="hunk"]   { color: #4FC3F7; background: rgba(79, 195, 247, 0.06); }
  .diff-row[data-kind="add"]    { color: #66BB6A; background: rgba(102, 187, 106, 0.08); }
  .diff-row[data-kind="del"]    { color: #EF5350; background: rgba(239, 83, 80, 0.08); }
  .diff-row[data-kind="ctx"]    { color: var(--la-text-body); }
  .diff-line-num {
    color: var(--la-text-dim);
    text-align: right;
    user-select: none;
    padding-right: 12px;
  }
  .diff-line-text {
    white-space: pre;
  }
  .diff-error {
    margin: 8px 18px 0;
    padding: 6px 10px;
    border-radius: var(--la-radius-md);
    background: var(--la-danger-bg);
    border: 1px solid var(--la-danger-stroke);
    color: var(--la-danger-text);
    font-size: 11px;
  }
  .diff-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 18px 14px;
    border-top: 1px solid #1e293b;
  }
  .diff-btn {
    padding: 6px 14px;
    font-size: 12px;
    font-family: inherit;
    font-weight: 600;
    border-radius: var(--la-radius-md);
    border: 1px solid transparent;
    cursor: pointer;
    transition: background var(--la-transition-fast), color var(--la-transition-fast);
  }
  .diff-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .diff-btn.primary {
    background: #d4a017;
    color: #0a0a0f;
  }
  .diff-btn.primary:hover:not(:disabled) {
    background: #f0c040;
    box-shadow: 0 0 12px rgba(255, 215, 0, 0.4);
  }
  @keyframes diff-fade-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
</style>
