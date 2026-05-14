<script lang="ts">
  // GitOpsPanel — branch dropdown, file-status list, commit dialog, push button.
  // Auto-refreshes git state every 30 s; clears the interval on destroy.
  import { gitStore, gitApi } from '$lib/stores';

  interface Props {
    /** Absolute path of the working directory for git operations. */
    cwd: string;
  }

  let { cwd }: Props = $props();

  // ── Local reactive state ────────────────────────────────────────────────────
  let commitMessage = $state('');

  // ── Store subscriptions — $state + $effect for proper Svelte 5 reactivity ────
  // $derived + IIFE-subscribe immediately unsubscribes, so $derived never registers
  // the store as a reactive dependency; the value is captured once and frozen.
  // $effect keeps the subscription alive until the component is destroyed.
  let currentBranch = $state('');
  let branches      = $state<string[]>([]);
  let fileStatuses  = $state<Array<{ path: string; status: string }>>([]);
  let loading       = $state(false);
  let errorMsg      = $state('');

  $effect(() => gitStore.currentBranch.subscribe(v => { currentBranch = v; }));
  $effect(() => gitStore.branches.subscribe(v => { branches = v; }));
  $effect(() => gitStore.fileStatuses.subscribe(v => { fileStatuses = v; }));
  $effect(() => gitStore.loading.subscribe(v => { loading = v; }));
  $effect(() => gitStore.error.subscribe(v => { errorMsg = v; }));

  // ── Auto-refresh on mount; clear on destroy ─────────────────────────────────
  $effect(() => {
    gitApi.status(cwd);
    const id = setInterval(() => { gitApi.status(cwd); }, 30_000);
    return () => { clearInterval(id); };
  });

  // ── Helpers ─────────────────────────────────────────────────────────────────

  function statusColor(s: string): string {
    if (s === 'M') return 'var(--la-semantic-warn)';
    if (s === 'A') return 'var(--la-semantic-ok)';
    if (s === 'D') return 'var(--la-semantic-error)';
    return 'var(--la-text-dim)';
  }

  function statusLabel(s: string): string {
    if (s === 'M') return 'modified';
    if (s === 'A') return 'added';
    if (s === 'D') return 'deleted';
    if (s === '?') return 'untracked';
    return s;
  }

  async function handleBranchSwitch(name: string) {
    if (name === currentBranch) return;
    await gitApi.branch('switch', name, cwd);
    await gitApi.status(cwd);
  }

  async function handleCommit() {
    const msg = commitMessage.trim();
    if (!msg || loading) return;
    await gitApi.commit(msg, cwd);
    commitMessage = '';
    await gitApi.status(cwd);
  }

  async function handlePush() {
    if (loading) return;
    await gitApi.push(cwd);
  }
</script>

<section class="git-panel" aria-label="Git operations">

  <!-- Error banner -->
  {#if errorMsg}
    <div class="error-banner" role="alert" aria-live="polite">
      {errorMsg}
    </div>
  {/if}

  <!-- Branch selector -->
  <div class="panel-section">
    <span class="section-label">BRANCH</span>
    <div class="branch-row">
      <select
        class="branch-select"
        value={currentBranch}
        aria-label="Select branch"
        onchange={(e) => handleBranchSwitch((e.currentTarget as HTMLSelectElement).value)}
        disabled={loading}
      >
        {#if currentBranch && !branches.includes(currentBranch)}
          <option value={currentBranch}>{currentBranch}</option>
        {/if}
        {#each branches as branch (branch)}
          <option value={branch}>{branch}</option>
        {/each}
      </select>

      <button
        class="icon-btn"
        onclick={() => gitApi.status(cwd)}
        aria-label="Refresh git status"
        disabled={loading}
      >
        {#if loading}
          <span class="spinner" aria-hidden="true"></span>
        {:else}
          ↻
        {/if}
      </button>
    </div>
  </div>

  <!-- File status list -->
  <div class="panel-section">
    <span class="section-label">CHANGES ({fileStatuses.length})</span>
    {#if fileStatuses.length === 0}
      <p class="empty-state">Working tree clean</p>
    {:else}
      <ul class="file-list" aria-label="Changed files">
        {#each fileStatuses as file (file.path)}
          <li class="file-row">
            <span
              class="status-badge"
              style:color={statusColor(file.status)}
              aria-label={statusLabel(file.status)}
              title={statusLabel(file.status)}
            >{file.status}</span>
            <span class="file-path" title={file.path}>{file.path}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <!-- Commit dialog -->
  <div class="panel-section">
    <span class="section-label">COMMIT</span>
    <div class="commit-row">
      <input
        class="commit-input"
        type="text"
        placeholder="Commit message…"
        aria-label="Commit message"
        bind:value={commitMessage}
        disabled={loading}
        onkeydown={(e) => { if (e.key === 'Enter') handleCommit(); }}
      />
      <button
        class="action-btn"
        onclick={handleCommit}
        aria-label="Commit all staged changes"
        disabled={loading || !commitMessage.trim()}
      >
        Commit All
      </button>
    </div>
  </div>

  <!-- Push -->
  <div class="panel-section push-section">
    <button
      class="action-btn push-btn"
      onclick={handlePush}
      aria-label="Push current branch to remote"
      disabled={loading}
    >
      {#if loading}
        <span class="spinner" aria-hidden="true"></span>
      {/if}
      Push
    </button>
  </div>

</section>

<style>
  .git-panel {
    display: flex;
    flex-direction: column;
    gap: 0;
    height: 100%;
    background: var(--la-bg-panel, #0f1117);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    color: var(--la-text-base, #b4bec8);
    overflow-y: auto;
  }

  /* ── Error banner ────────────────────────────────────────────────────────── */
  .error-banner {
    padding: 8px 12px;
    background: rgba(239, 68, 68, 0.12);
    border-bottom: 1px solid var(--la-semantic-error, #ef4444);
    color: var(--la-semantic-error, #ef4444);
    font-size: 11px;
    line-height: 1.4;
    word-break: break-word;
  }

  /* ── Section chrome ──────────────────────────────────────────────────────── */
  .panel-section {
    padding: 10px 12px;
    border-bottom: 1px solid var(--la-hair-base, #25282d);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-label {
    font-size: 10px;
    letter-spacing: var(--la-tk-loose, 0.18em);
    color: var(--la-text-mute, #5a6472);
    text-transform: uppercase;
    user-select: none;
  }

  /* ── Branch row ──────────────────────────────────────────────────────────── */
  .branch-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .branch-select {
    flex: 1;
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    padding: 4px 6px;
    cursor: pointer;
    outline: none;
    appearance: none;
    -webkit-appearance: none;
  }

  .branch-select:focus-visible {
    outline: var(--la-focus-ring-width, 2px) solid var(--la-focus-ring, #FFD700);
    outline-offset: var(--la-focus-ring-offset, 2px);
  }

  .branch-select:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* ── File list ───────────────────────────────────────────────────────────── */
  .empty-state {
    color: var(--la-text-mute, #5a6472);
    font-size: 11px;
    margin: 0;
    padding: 4px 0;
  }

  .file-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 200px;
    overflow-y: auto;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .status-badge {
    width: 14px;
    font-weight: 700;
    text-align: center;
    flex-shrink: 0;
    font-size: 11px;
    line-height: 1;
  }

  .file-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--la-text-base, #b4bec8);
    font-size: 11px;
  }

  /* ── Commit row ──────────────────────────────────────────────────────────── */
  .commit-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .commit-input {
    flex: 1;
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    padding: 5px 8px;
    outline: none;
    min-width: 0;
  }

  .commit-input::placeholder {
    color: var(--la-text-mute, #5a6472);
  }

  .commit-input:focus-visible {
    outline: var(--la-focus-ring-width, 2px) solid var(--la-focus-ring, #FFD700);
    outline-offset: var(--la-focus-ring-offset, 2px);
  }

  .commit-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* ── Push section ────────────────────────────────────────────────────────── */
  .push-section {
    border-bottom: none;
  }

  /* ── Shared action button ────────────────────────────────────────────────── */
  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-strong, #3a3f47);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    letter-spacing: var(--la-tk-mid, 0.08em);
    cursor: pointer;
    transition: background var(--la-transition-fast, 120ms ease);
    white-space: nowrap;
  }

  .action-btn:hover:not(:disabled) {
    background: var(--la-struct-primary, #00c8ff);
    color: var(--la-bg-base, #0a0a0f);
    border-color: var(--la-struct-primary, #00c8ff);
  }

  .action-btn:focus-visible {
    outline: var(--la-focus-ring-width, 2px) solid var(--la-focus-ring, #FFD700);
    outline-offset: var(--la-focus-ring-offset, 2px);
  }

  .action-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .push-btn {
    width: 100%;
    justify-content: center;
  }

  /* ── Icon / refresh button ───────────────────────────────────────────────── */
  .icon-btn {
    background: none;
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-dim, #96a2ae);
    cursor: pointer;
    padding: 4px 7px;
    font-size: 14px;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color var(--la-transition-fast, 120ms ease);
    flex-shrink: 0;
  }

  .icon-btn:hover:not(:disabled) {
    color: var(--la-text-bright, #f1f5f9);
    background: var(--la-bg-elevated, #1a2030);
  }

  .icon-btn:focus-visible {
    outline: var(--la-focus-ring-width, 2px) solid var(--la-focus-ring, #FFD700);
    outline-offset: var(--la-focus-ring-offset, 2px);
  }

  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* ── Loading spinner ─────────────────────────────────────────────────────── */
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .spinner {
    display: inline-block;
    width: 10px;
    height: 10px;
    border: 2px solid var(--la-text-mute, #5a6472);
    border-top-color: var(--la-struct-primary, #00c8ff);
    border-radius: 50%;
    animation: spin 600ms linear infinite;
    flex-shrink: 0;
  }

  /* ── Reduced motion ──────────────────────────────────────────────────────── */
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
      border-top-color: var(--la-struct-primary, #00c8ff);
    }
  }
</style>
