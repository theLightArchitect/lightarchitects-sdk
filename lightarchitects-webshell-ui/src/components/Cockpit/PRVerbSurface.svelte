<script lang="ts">
  import { authHeaders } from '$lib/auth';
  import { selectedTarget } from '$lib/cockpit/stores';
  import ForkConfirmationModal from './ForkConfirmationModal.svelte';

  type ReviewEvent = 'APPROVE' | 'REQUEST_CHANGES' | 'COMMENT';

  interface Props {
    owner: string;
    repo: string;
    prNumber: number;
    headSha: string;
  }

  let { owner, repo, prNumber, headSha }: Props = $props();

  let event     = $state<ReviewEvent>('COMMENT');
  let body      = $state('');
  let submitting = $state(false);
  let result    = $state<'ok' | 'error' | 'precondition' | null>(null);
  let errorMsg  = $state('');
  let showConfirm = $state(false);

  const EVENTS: { key: ReviewEvent; label: string }[] = [
    { key: 'COMMENT',          label: 'COMMENT' },
    { key: 'APPROVE',          label: 'APPROVE' },
    { key: 'REQUEST_CHANGES',  label: 'REQUEST CHANGES' },
  ];

  function trySubmit() {
    if (event === 'APPROVE') {
      showConfirm = true;
    } else {
      void doSubmit();
    }
  }

  async function doSubmit() {
    showConfirm = false;
    if (submitting) return;
    submitting = true;
    result = null;
    errorMsg = '';
    try {
      const res = await fetch(
        `/api/github-proxy/pr/${owner}/${repo}/${prNumber}/review`,
        {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'If-Match': `"${headSha}"`,
            ...authHeaders(),
          },
          body: JSON.stringify({ event, body }),
        },
      );
      if (res.status === 412) {
        result = 'precondition';
        errorMsg = 'PR updated since you loaded it — reload and re-review.';
      } else if (res.status === 403) {
        result = 'error';
        errorMsg = 'Origin not allowed.';
      } else if (!res.ok) {
        result = 'error';
        errorMsg = `Submit failed: ${res.status}`;
      } else {
        result = 'ok';
        body = '';
        // Deselect target after a successful review submission
        setTimeout(() => {
          selectedTarget.set(null);
          result = null;
        }, 2000);
      }
    } catch (e) {
      result = 'error';
      errorMsg = e instanceof Error ? e.message : 'network error';
    } finally {
      submitting = false;
    }
  }

  const canSubmit = $derived(
    !submitting &&
    headSha.length > 0 &&
    (event !== 'REQUEST_CHANGES' || body.trim().length > 0),
  );
</script>

{#if showConfirm}
  <ForkConfirmationModal
    {headSha}
    onConfirm={() => { void doSubmit(); }}
    onCancel={() => { showConfirm = false; }}
  />
{/if}

<div class="verb-surface">
  <!-- Event type selector -->
  <div class="verb-tabs" role="tablist" aria-label="Review type">
    {#each EVENTS as ev}
      <button
        role="tab"
        class="verb-tab"
        class:verb-tab-active={event === ev.key}
        class:verb-tab-approve={ev.key === 'APPROVE' && event === ev.key}
        class:verb-tab-req={ev.key === 'REQUEST_CHANGES' && event === ev.key}
        aria-selected={event === ev.key}
        onclick={() => { event = ev.key; }}
      >{ev.label}</button>
    {/each}
  </div>

  <!-- Body textarea -->
  <textarea
    class="verb-body"
    placeholder={event === 'APPROVE'
      ? 'Optional comment for approval…'
      : event === 'REQUEST_CHANGES'
        ? 'Required: describe changes requested…'
        : 'Comment on this PR…'}
    bind:value={body}
    rows={4}
    disabled={submitting}
    aria-label="Review body"
  ></textarea>

  <!-- Submit row -->
  <div class="verb-actions">
    {#if result === 'ok'}
      <span class="verb-ok">✓ review submitted</span>
    {:else if result === 'precondition'}
      <span class="verb-warn">{errorMsg}</span>
    {:else if result === 'error'}
      <span class="verb-err">{errorMsg}</span>
    {/if}

    <button
      class="verb-submit"
      class:verb-submit-approve={event === 'APPROVE'}
      class:verb-submit-req={event === 'REQUEST_CHANGES'}
      onclick={trySubmit}
      disabled={!canSubmit}
      aria-label="Submit review"
    >
      {submitting ? 'SUBMITTING…' : event === 'APPROVE' ? 'APPROVE PR' : event === 'REQUEST_CHANGES' ? 'REQUEST CHANGES' : 'COMMENT'}
    </button>
  </div>
</div>

<style>
  .verb-surface {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--la-hair-base);
  }

  .verb-tabs {
    display: flex;
    gap: 0;
  }

  .verb-tab {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.07em;
    padding: 3px 8px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
  }

  .verb-tab:not(:first-child) { border-left: none; }
  .verb-tab:first-child { border-radius: 2px 0 0 2px; }
  .verb-tab:last-child  { border-radius: 0 2px 2px 0; }

  .verb-tab-active {
    background: var(--la-struct-primary);
    color: var(--la-bg-base);
    border-color: var(--la-struct-primary);
  }

  .verb-tab-approve {
    background: var(--la-semantic-ok);
    color: var(--la-bg-base);
    border-color: var(--la-semantic-ok);
  }

  .verb-tab-req {
    background: var(--la-semantic-warn);
    color: var(--la-bg-base);
    border-color: var(--la-semantic-warn);
  }

  .verb-body {
    width: 100%;
    background: var(--la-bg-base);
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-base);
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    padding: 6px 8px;
    resize: vertical;
    box-sizing: border-box;
    outline: none;
  }

  .verb-body:focus { border-color: var(--la-struct-primary); }

  .verb-body::placeholder { color: var(--la-text-mute); }

  .verb-body:disabled { opacity: 0.5; }

  .verb-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: flex-end;
    flex-wrap: wrap;
  }

  .verb-ok   { font-size: 9px; color: var(--la-semantic-ok);    font-family: var(--la-font-mono, monospace); }
  .verb-warn { font-size: 9px; color: var(--la-semantic-warn);  font-family: var(--la-font-mono, monospace); }
  .verb-err  { font-size: 9px; color: var(--la-semantic-error); font-family: var(--la-font-mono, monospace); }

  .verb-submit {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 4px 12px;
    border: 1px solid var(--la-struct-primary);
    background: transparent;
    color: var(--la-struct-primary);
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
  }

  .verb-submit:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-struct-primary) 12%, transparent);
  }

  .verb-submit:disabled { opacity: 0.4; cursor: default; }

  .verb-submit-approve {
    border-color: var(--la-semantic-ok);
    color: var(--la-semantic-ok);
  }

  .verb-submit-approve:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-semantic-ok) 12%, transparent);
  }

  .verb-submit-req {
    border-color: var(--la-semantic-warn);
    color: var(--la-semantic-warn);
  }

  .verb-submit-req:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-semantic-warn) 12%, transparent);
  }
</style>
