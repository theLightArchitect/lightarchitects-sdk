<script lang="ts">
  // PullRequest screen — shows PRCreateForm at /pr/new or PRReviewSurface
  // at /pr/:number. The :number param is injected by App.svelte as params.number.
  //
  // When showing the review surface we load the diff via POST /api/git/diff
  // using the `cwd` from a URL param or a '.'-based default.
  import { onMount } from 'svelte';
  import PRCreateForm from '../components/git/PRCreateForm.svelte';
  import PRReviewSurface from '../components/git/PRReviewSurface.svelte';
  import { authHeaders } from '$lib/auth';

  // Params injected by App.svelte.
  let { params = {} }: { params?: Record<string, string> } = $props();

  // Detect mode: if params.number is present we're on /pr/:number.
  const prNumber = $derived(params.number ? parseInt(params.number, 10) : null);
  const isReview = $derived(prNumber !== null && !isNaN(prNumber as number));

  // Owner/repo — derived from URL params or sensible defaults.
  // In a live system these would be parsed from `git remote get-url origin`.
  const owner = $derived(params.owner ?? 'TheLightArchitects');
  const repo  = $derived(params.repo  ?? '');
  const cwd   = $derived(params.cwd   ?? '.');

  // ── Review mode: load diff ──────────────────────────────────────────────────

  let diff = $state('');
  let diffLoading = $state(false);
  let diffError = $state<string | null>(null);

  async function loadDiff(dir: string) {
    diffLoading = true;
    diffError = null;
    try {
      const res = await fetch('/api/git/diff', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ staged: false, cwd: dir }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`Diff load failed: ${res.status} ${text}`);
      }
      const data = (await res.json()) as { diff?: string };
      diff = data.diff ?? '';
    } catch (e) {
      diffError = e instanceof Error ? e.message : 'Failed to load diff';
      diff = '';
    } finally {
      diffLoading = false;
    }
  }

  // Load diff when entering review mode.
  $effect(() => {
    if (isReview) void loadDiff(cwd);
  });
</script>

<div class="pr-screen" data-testid="pr-screen">
  {#if isReview}
    <!-- /pr/:number → review surface -->
    <div class="review-wrap">
      {#if diffLoading}
        <div class="loading-state">
          <div class="spinner" aria-hidden="true"></div>
          <span class="loading-text">Loading diff…</span>
        </div>
      {:else if diffError}
        <div class="error-banner" role="alert">
          {diffError}
          <button
            class="retry-btn"
            type="button"
            onclick={() => loadDiff(cwd)}
          >Retry</button>
        </div>
      {/if}
      <!-- Always render the surface (even with empty diff) so E2E can assert it -->
      <PRReviewSurface
        {diff}
        prNumber={prNumber as number}
        {owner}
        {repo}
        {cwd}
      />
    </div>
  {:else}
    <!-- /pr/new → create form -->
    <div class="create-wrap">
      <PRCreateForm {owner} {repo} {cwd} />
    </div>
  {/if}
</div>

<style>
  .pr-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--la-bg, #0a0a0f);
    overflow: hidden;
  }

  /* ── Review wrap ─────────────────────────────────────────────────────────── */
  .review-wrap {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .loading-state {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--la-hair-base, #25282d);
  }

  .loading-text {
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    color: var(--la-text-mute, #5a6472);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
    background: rgba(239, 68, 68, 0.1);
    border-bottom: 1px solid var(--la-semantic-error, #ef4444);
    color: var(--la-semantic-error, #ef4444);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    flex-shrink: 0;
    word-break: break-word;
  }

  .retry-btn {
    margin-left: auto;
    padding: 3px 10px;
    background: none;
    border: 1px solid var(--la-semantic-error, #ef4444);
    color: var(--la-semantic-error, #ef4444);
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .retry-btn:hover {
    background: rgba(239, 68, 68, 0.1);
  }

  .retry-btn:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  /* ── Create wrap ─────────────────────────────────────────────────────────── */
  .create-wrap {
    flex: 1;
    overflow-y: auto;
    display: flex;
    justify-content: center;
    padding: 20px;
    box-sizing: border-box;
  }

  /* ── Spinner ─────────────────────────────────────────────────────────────── */
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .spinner {
    display: inline-block;
    width: 14px;
    height: 14px;
    border: 2px solid var(--la-hair-base, #25282d);
    border-top-color: var(--la-struct-primary, #00c8ff);
    border-radius: 50%;
    animation: spin 600ms linear infinite;
    flex-shrink: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner { animation: none; }
  }
</style>
