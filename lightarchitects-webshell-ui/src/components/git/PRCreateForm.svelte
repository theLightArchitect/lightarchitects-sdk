<script lang="ts">
  // PRCreateForm — GitHub Pull Request creation form.
  // Fields: title (required), body (optional, markdown), base branch, head branch (read-only).
  // Submits to POST /api/git/pr/create; shows PR URL on success.
  import { gitStore, gitApi } from '$lib/stores';
  import { authHeaders } from '$lib/auth';

  interface Props {
    /** GitHub owner (org or user). */
    owner: string;
    /** GitHub repository name. */
    repo: string;
    /** Working directory for git operations. */
    cwd: string;
  }

  let { owner, repo, cwd }: Props = $props();

  // ── Local state ────────────────────────────────────────────────────────────
  let title = $state('');
  let body = $state('');
  let base = $state('main');
  let showPreview = $state(false);
  let submitting = $state(false);
  let prUrl = $state<string | null>(null);
  let submitError = $state<string | null>(null);

  // ── Store subscriptions — $state + $effect for proper Svelte 5 reactivity ────
  let currentBranch = $state('');
  let branches      = $state<string[]>([]);

  $effect(() => gitStore.currentBranch.subscribe(v => { currentBranch = v; }));
  $effect(() => gitStore.branches.subscribe(v => { branches = v; }));

  // ── Bootstrap branches on mount ────────────────────────────────────────────
  $effect(() => {
    if (cwd) void gitApi.status(cwd);
  });

  // ── Derived ────────────────────────────────────────────────────────────────
  const canSubmit = $derived(title.trim().length > 0 && !submitting);

  // ── Helpers ────────────────────────────────────────────────────────────────

  async function handleSubmit() {
    if (!canSubmit) return;
    submitting = true;
    submitError = null;
    prUrl = null;
    try {
      const res = await fetch('/api/git/pr/create', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          owner,
          repo,
          title: title.trim(),
          body,
          base,
          head: currentBranch,
          cwd,
        }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`PR create failed: ${res.status} ${text}`);
      }
      const data = (await res.json()) as { url?: string; html_url?: string };
      prUrl = data.url ?? data.html_url ?? null;
    } catch (e) {
      submitError = e instanceof Error ? e.message : 'Failed to create PR';
    } finally {
      submitting = false;
    }
  }
</script>

<div class="pr-form" data-testid="pr-create-form">
  <header class="form-header">
    <span class="form-title">Open Pull Request</span>
    <span class="form-repo">{owner}/{repo}</span>
  </header>

  {#if prUrl}
    <div class="success-banner" role="status">
      Pull request created: <a class="pr-link" href={prUrl} target="_blank" rel="noreferrer">{prUrl}</a>
    </div>
  {/if}

  {#if submitError}
    <div class="error-banner" role="alert">{submitError}</div>
  {/if}

  <!-- Title -->
  <div class="field-group">
    <label class="field-label" for="pr-title">Title <span class="required" aria-hidden="true">*</span></label>
    <input
      id="pr-title"
      class="field-input"
      type="text"
      placeholder="Summary of this change"
      aria-label="Pull request title"
      aria-required="true"
      data-testid="pr-title-input"
      bind:value={title}
    />
  </div>

  <!-- Body -->
  <div class="field-group">
    <div class="label-row">
      <label class="field-label" for="pr-body">Description</label>
      <button
        class="toggle-btn"
        type="button"
        onclick={() => { showPreview = !showPreview; }}
        aria-pressed={showPreview}
      >{showPreview ? 'Edit' : 'Preview'}</button>
    </div>

    {#if showPreview}
      <div class="body-preview" aria-label="Markdown preview">
        {#if body.trim()}
          <pre class="preview-text">{body}</pre>
        {:else}
          <span class="preview-empty">No description provided.</span>
        {/if}
      </div>
    {:else}
      <textarea
        id="pr-body"
        class="field-textarea"
        placeholder="Describe your changes (markdown supported)"
        aria-label="Pull request description"
        data-testid="pr-body-textarea"
        rows={6}
        bind:value={body}
      ></textarea>
    {/if}
  </div>

  <!-- Branch row: base (select) + head (read-only) -->
  <div class="branch-row">
    <div class="field-group branch-field">
      <label class="field-label" for="pr-base">Base branch</label>
      <select
        id="pr-base"
        class="field-select"
        aria-label="Base branch"
        bind:value={base}
      >
        {#each branches as branch (branch)}
          <option value={branch}>{branch}</option>
        {/each}
        {#if branches.length === 0}
          <option value="main">main</option>
        {/if}
      </select>
    </div>

    <div class="field-group branch-field">
      <label class="field-label" for="pr-head">Head branch</label>
      <input
        id="pr-head"
        class="field-input"
        type="text"
        value={currentBranch || 'loading…'}
        aria-label="Head branch (current)"
        readonly
        aria-readonly="true"
      />
    </div>
  </div>

  <!-- Submit -->
  <div class="actions">
    <button
      class="submit-btn"
      type="button"
      data-testid="pr-submit-btn"
      onclick={handleSubmit}
      disabled={!canSubmit}
      aria-disabled={!canSubmit}
    >
      {#if submitting}
        <span class="spinner" aria-hidden="true"></span>
      {/if}
      Create Pull Request
    </button>
  </div>
</div>

<style>
  .pr-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
    max-width: 680px;
    background: var(--la-bg-panel, #0f1117);
    font-family: var(--la-font-mono, monospace);
    font-size: 13px;
    color: var(--la-text-base, #b4bec8);
  }

  .form-header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    border-bottom: 1px solid var(--la-hair-base, #25282d);
    padding-bottom: 12px;
  }

  .form-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--la-text-bright, #f1f5f9);
    letter-spacing: var(--la-tk-mid, 0.08em);
  }

  .form-repo {
    font-size: 11px;
    color: var(--la-text-mute, #5a6472);
  }

  /* ── Banners ────────────────────────────────────────────────────────────── */
  .success-banner {
    padding: 10px 12px;
    background: rgba(74, 222, 128, 0.10);
    border: 1px solid var(--la-semantic-ok, #4ade80);
    color: var(--la-semantic-ok, #4ade80);
    font-size: 12px;
    border-radius: 2px;
    word-break: break-all;
  }

  .pr-link {
    color: var(--la-struct-primary, #00c8ff);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .error-banner {
    padding: 10px 12px;
    background: rgba(239, 68, 68, 0.10);
    border: 1px solid var(--la-semantic-error, #ef4444);
    color: var(--la-semantic-error, #ef4444);
    font-size: 12px;
    border-radius: 2px;
    word-break: break-word;
  }

  /* ── Fields ─────────────────────────────────────────────────────────────── */
  .field-group {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .field-label {
    font-size: 10px;
    letter-spacing: var(--la-tk-loose, 0.18em);
    text-transform: uppercase;
    color: var(--la-text-mute, #5a6472);
    user-select: none;
  }

  .required {
    color: var(--la-semantic-error, #ef4444);
    margin-left: 2px;
  }

  .field-input,
  .field-select,
  .field-textarea {
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 13px;
    padding: 6px 8px;
    outline: none;
    resize: none;
    width: 100%;
    box-sizing: border-box;
  }

  .field-input:focus-visible,
  .field-select:focus-visible,
  .field-textarea:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .field-input[readonly] {
    opacity: 0.6;
    cursor: default;
  }

  /* ── Branch row ─────────────────────────────────────────────────────────── */
  .branch-row {
    display: flex;
    gap: 12px;
  }

  .branch-field {
    flex: 1;
  }

  /* ── Label + toggle row ─────────────────────────────────────────────────── */
  .label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .toggle-btn {
    background: none;
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-dim, #96a2ae);
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    padding: 2px 8px;
    cursor: pointer;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    transition: color 120ms ease, border-color 120ms ease;
  }

  .toggle-btn:hover {
    color: var(--la-text-bright, #f1f5f9);
    border-color: var(--la-text-dim, #96a2ae);
  }

  .toggle-btn:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  /* ── Markdown preview ────────────────────────────────────────────────────── */
  .body-preview {
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-base, #25282d);
    padding: 10px 12px;
    min-height: 120px;
  }

  .preview-text {
    margin: 0;
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    color: var(--la-text-base, #b4bec8);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .preview-empty {
    color: var(--la-text-mute, #5a6472);
    font-size: 12px;
    font-style: italic;
  }

  /* ── Actions ────────────────────────────────────────────────────────────── */
  .actions {
    display: flex;
    justify-content: flex-end;
  }

  .submit-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 20px;
    background: var(--la-struct-primary, #00c8ff);
    border: none;
    color: var(--la-bg-base, #0a0a0f);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: var(--la-tk-mid, 0.08em);
    cursor: pointer;
    transition: opacity 120ms ease;
  }

  .submit-btn:hover:not(:disabled) {
    opacity: 0.85;
  }

  .submit-btn:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .submit-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid rgba(10, 10, 15, 0.3);
    border-top-color: var(--la-bg-base, #0a0a0f);
    border-radius: 50%;
    animation: spin 600ms linear infinite;
    flex-shrink: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner { animation: none; }
  }
</style>
