<script lang="ts">
  /**
   * DiffPreview — operator-gated approval modal (E5 extension).
   *
   * Handles two event kinds:
   *  1. `la:fs-mutation-pending` — FS mutation gate (unified diff view)
   *  2. `la:permission-request`  — generic tool-call approval (input_preview view)
   *
   * Both resolve via POST /api/dispatch/:build_id/fs-approve or fs-reject.
   */
  import {
    type FsMutationPendingEvent,
    approveMutation,
    rejectMutation,
  } from '$lib/diff-preview';
  import { DOMAIN_AGENT_COLORS } from '$lib/design-tokens';

  // ── Discriminated union covering both approval kinds ─────────────────────────

  interface FsMutationPending {
    kind: 'fs_mutation';
    dispatch_id: string;
    mutation_id: string;
    agent: string;
    file_path: string;
    tool: string;
    diff_unified: string;
  }

  interface PermissionRequestPending {
    kind: 'permission_request';
    dispatch_id: string;
    /** call_id from the backend — used as mutation_id for the approval endpoint. */
    call_id: string;
    input_preview: string;
    risk_tier: 'low' | 'medium' | 'high' | 'critical';
  }

  type PendingRequest = FsMutationPending | PermissionRequestPending;

  const RISK_COLORS: Record<string, string> = {
    low:      '#22d3ee',
    medium:   '#facc15',
    high:     '#f97316',
    critical: '#ef4444',
  };

  let pending = $state<PendingRequest | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // First-encounter coachmark (#71): shown once, then dismissed forever.
  const SEEN_KEY = 'la_diff_preview_seen';
  let showCoachmark = $state(false);

  $effect(() => {
    if (pending && !localStorage.getItem(SEEN_KEY)) {
      const t = setTimeout(() => { showCoachmark = true; }, 400);
      return () => { clearTimeout(t); showCoachmark = false; };
    }
  });

  function dismissCoachmark() {
    showCoachmark = false;
    localStorage.setItem(SEEN_KEY, '1');
  }

  $effect(() => {
    function onMutationPending(e: Event) {
      const d = (e as CustomEvent<FsMutationPendingEvent>).detail;
      if (!pending) {
        pending = {
          kind: 'fs_mutation',
          dispatch_id: d.dispatch_id,
          mutation_id: d.mutation_id,
          agent: d.agent,
          file_path: d.file_path,
          tool: d.tool,
          diff_unified: d.diff_unified,
        };
        error = null;
      }
    }

    function onPermissionRequest(e: Event) {
      const d = (e as CustomEvent<{ dispatch_id: string; call_id: string; input_preview: string; risk_tier: string }>).detail;
      if (!pending) {
        pending = {
          kind: 'permission_request',
          dispatch_id: d.dispatch_id,
          call_id: d.call_id,
          input_preview: d.input_preview,
          risk_tier: (d.risk_tier as PermissionRequestPending['risk_tier']) ?? 'medium',
        };
        error = null;
      }
    }

    window.addEventListener('la:fs-mutation-pending', onMutationPending);
    window.addEventListener('la:permission-request', onPermissionRequest);
    return () => {
      window.removeEventListener('la:fs-mutation-pending', onMutationPending);
      window.removeEventListener('la:permission-request', onPermissionRequest);
    };
  });

  // ── Derived values ────────────────────────────────────────────────────────────

  let diffRows = $derived.by(() => {
    if (!pending || pending.kind !== 'fs_mutation') return [];
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
    pending && pending.kind === 'fs_mutation' &&
    (DOMAIN_AGENT_COLORS as Record<string, string>)[pending.agent]
      ? (DOMAIN_AGENT_COLORS as Record<string, string>)[pending.agent]
      : '#FFD700',
  );

  let riskColor = $derived(
    pending && pending.kind === 'permission_request'
      ? (RISK_COLORS[pending.risk_tier] ?? '#FFD700')
      : '#FFD700',
  );

  // ── Actions ───────────────────────────────────────────────────────────────────

  function mutationId(p: PendingRequest): string {
    return p.kind === 'fs_mutation' ? p.mutation_id : p.call_id;
  }

  async function approve() {
    if (!pending || busy) return;
    busy = true;
    error = null;
    try {
      await approveMutation(pending.dispatch_id, mutationId(pending));
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
      await rejectMutation(pending.dispatch_id, mutationId(pending), reason);
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
    data-kind={pending.kind}
  >
    <div class="diff-modal">

      {#if pending.kind === 'fs_mutation'}
        <!-- ── FS mutation view: unified diff ───────────────────────────── -->
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

      {:else}
        <!-- ── Permission request view: tool-call approval ──────────────── -->
        <header class="diff-header">
          <div class="diff-title-row">
            <span
              class="perm-risk-badge"
              style="background-color: {riskColor};"
              aria-label="risk tier {pending.risk_tier}"
              data-testid="risk-badge"
            >{pending.risk_tier.toUpperCase()}</span>
            <strong id="diff-title">Tool permission required</strong>
          </div>
          <p class="diff-explainer">
            The agent is requesting permission to execute a tool call.
            Review the details and Approve or Deny. <kbd>Esc</kbd> denies.
          </p>
        </header>

        <div class="perm-body" data-testid="permission-preview">
          <pre class="perm-preview">{pending.input_preview}</pre>
        </div>
      {/if}

      {#if error}
        <p class="diff-error" role="alert">{error}</p>
      {/if}

      <footer class="diff-actions">
        <button
          class="diff-btn la-destructive"
          onclick={() => reject('operator-rejected')}
          disabled={busy}
          data-testid="reject-btn"
        >
          {pending.kind === 'fs_mutation' ? 'Reject' : 'Deny'}
        </button>
        <button
          class="diff-btn primary"
          onclick={approve}
          disabled={busy}
          data-testid="approve-btn"
        >
          {busy ? 'Working…' : pending.kind === 'fs_mutation' ? 'Approve & commit' : 'Approve'}
        </button>
      </footer>
    </div>
  </div>
{/if}

<!-- First-encounter coachmark (#71) — appears once, dismissed on click or after 8s -->
{#if showCoachmark}
  <div
    class="coachmark"
    role="button"
    tabindex="0"
    aria-label="Dismiss write-intercept notice"
    onclick={dismissCoachmark}
    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); dismissCoachmark(); } }}
  >
    <div class="coachmark-inner">
      <span class="coachmark-icon">⚠</span>
      <div class="coachmark-text">
        <strong>Agent write intercepted</strong>
        <p>Every file change passes through here first. Approve to commit, Reject to abort. You stay in control.</p>
      </div>
      <button
        class="coachmark-dismiss"
        aria-label="Dismiss"
        onclick={(e) => { e.stopPropagation(); dismissCoachmark(); }}
      >Got it</button>
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
    background: var(--la-drawer-bg);
    border: 1px solid var(--la-drawer-border);
    border-radius: 0;
    box-shadow:
      0 12px 32px rgba(0, 0, 0, 0.5),
      0 0 0 1px rgba(255, 215, 0, 0.08);
    color: var(--la-text-body);
    font-family: var(--la-font-chrome);
  }
  .diff-header {
    padding: 14px 18px 8px;
    border-bottom: 1px solid var(--la-drawer-border);
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
    background: var(--la-bg-elev-2);
    border-radius: 0;
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
    background: var(--la-bg-elev-2);
    padding: 1px 5px;
    border-radius: 0;
    font-size: 10px;
  }
  .diff-body {
    flex: 1;
    overflow: auto;
    padding: 8px 0;
    background: var(--la-bg-frame);
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
    border-radius: 0;
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
    border-top: 1px solid var(--la-drawer-border);
  }
  .diff-btn {
    padding: 6px 14px;
    font-size: 12px;
    font-family: inherit;
    font-weight: 600;
    border-radius: 0;
    border: 1px solid transparent;
    cursor: pointer;
    transition: background var(--la-transition-fast), color var(--la-transition-fast);
  }
  .diff-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .diff-btn.primary {
    background: var(--la-focus-ring);
    color: var(--la-bg-frame);
  }
  .diff-btn.primary:hover:not(:disabled) {
    background: var(--la-focus-ring);
    box-shadow: 0 0 12px rgba(255, 215, 0, 0.4);
  }
  @keyframes diff-fade-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }

  /* ── Permission-request view ── */
  .perm-risk-badge {
    display: inline-block;
    padding: 1px 7px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-bg-frame);
    border-radius: 0;
    flex-shrink: 0;
  }
  .perm-body {
    flex: 1;
    overflow: auto;
    padding: 14px 18px;
    background: var(--la-bg-frame);
  }
  .perm-preview {
    margin: 0;
    font-family: var(--la-font-mono);
    font-size: 12px;
    line-height: 1.55;
    color: var(--la-text-body);
    white-space: pre-wrap;
    word-break: break-word;
  }

  /* ── First-encounter coachmark (#71) ── */
  .coachmark {
    position: fixed;
    bottom: 80px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 100;
    animation: coachmark-rise 0.35s var(--la-ease-mech) both;
    cursor: pointer;
  }
  .coachmark-inner {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 16px;
    background: var(--la-bg-frame);
    border: 1px solid var(--la-focus-ring);
    border-radius: 0;
    box-shadow: 0 4px 24px rgba(0,0,0,0.5), 0 0 0 1px rgba(255,215,0,0.15);
    max-width: 380px;
    font-family: var(--la-font-chrome);
  }
  .coachmark-icon {
    font-size: 16px;
    line-height: 1;
    color: var(--la-focus-ring);
    flex-shrink: 0;
    margin-top: 1px;
  }
  .coachmark-text strong {
    display: block;
    font-size: 12px;
    font-weight: 600;
    color: var(--la-text-stark);
    margin-bottom: 3px;
  }
  .coachmark-text p {
    margin: 0;
    font-size: 11px;
    color: var(--la-text-base);
    line-height: 1.5;
  }
  .coachmark-dismiss {
    flex-shrink: 0;
    padding: 3px 10px;
    margin-top: 2px;
    background: var(--la-focus-ring);
    border: none;
    border-radius: 0;
    color: var(--la-bg-frame);
    font-family: var(--la-font-chrome);
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
    align-self: center;
    white-space: nowrap;
    transition: opacity var(--la-t-snap) var(--la-ease-mech);
  }
  .coachmark-dismiss:hover { opacity: 0.85; }

  @keyframes coachmark-rise {
    from { opacity: 0; transform: translateX(-50%) translateY(12px); }
    to   { opacity: 1; transform: translateX(-50%) translateY(0); }
  }
</style>
