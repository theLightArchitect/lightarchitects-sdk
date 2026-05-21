<script lang="ts">
  interface Props {
    headSha: string;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let { headSha, onConfirm, onCancel }: Props = $props();

  function shortSha(sha: string): string {
    return sha.slice(0, 12);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="modal-backdrop" onclick={onCancel}></div>

<div class="modal" role="alertdialog" aria-modal="true" aria-label="Confirm review submission">
  <div class="modal-title">CONFIRM APPROVE</div>

  <div class="modal-body">
    <p class="modal-warn">
      You are about to approve this PR. Verify the HEAD SHA matches what you reviewed:
    </p>
    <code class="modal-sha">{shortSha(headSha)}</code>
    <p class="modal-note">
      If the PR has received new commits since you began reviewing, the submission will be
      rejected with <code>412 Precondition Failed</code> — reload and re-review.
    </p>
  </div>

  <div class="modal-actions">
    <button class="btn-confirm" onclick={onConfirm}>APPROVE NOW</button>
    <button class="btn-cancel"  onclick={onCancel}>CANCEL</button>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: rgba(0, 0, 0, 0.6);
  }

  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 201;
    width: min(420px, 90vw);
    background: var(--la-bg-panel);
    border: 1px solid var(--la-semantic-warn);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .modal-title {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.10em;
    color: var(--la-semantic-warn);
    font-family: var(--la-font-mono, monospace);
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .modal-warn {
    font-size: 10px;
    color: var(--la-text-base);
    font-family: var(--la-font-mono, monospace);
    margin: 0;
  }

  .modal-sha {
    font-size: 13px;
    font-weight: 700;
    color: var(--la-struct-primary);
    font-family: var(--la-font-mono, monospace);
    letter-spacing: 0.08em;
  }

  .modal-note {
    font-size: 9px;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    margin: 0;
  }

  .modal-note code {
    color: var(--la-semantic-warn);
    font-family: inherit;
  }

  .modal-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .btn-confirm, .btn-cancel {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 5px 12px;
    border: 1px solid;
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
    background: transparent;
  }

  .btn-confirm {
    border-color: var(--la-semantic-ok);
    color: var(--la-semantic-ok);
  }

  .btn-confirm:hover {
    background: color-mix(in srgb, var(--la-semantic-ok) 12%, transparent);
  }

  .btn-cancel {
    border-color: var(--la-hair-base);
    color: var(--la-text-mute);
  }

  .btn-cancel:hover {
    color: var(--la-text-base);
  }
</style>
