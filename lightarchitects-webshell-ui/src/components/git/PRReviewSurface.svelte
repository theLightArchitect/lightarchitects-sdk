<script lang="ts">
  // PRReviewSurface — unified-diff renderer with inline comment overlay.
  // Parses a `diff` string (git unified diff format) into hunks, renders
  // + lines green, - lines red, context lines gray. Each + or - line
  // is clickable to open an inline comment textarea.
  // Submits to POST /api/git/pr/review using the position-based GitHub API payload.
  import { authHeaders } from '$lib/auth';

  interface Props {
    /** Unified diff string (output of `git diff`). */
    diff: string;
    /** PR number on GitHub. */
    prNumber: number;
    /** GitHub owner (org or user). */
    owner: string;
    /** GitHub repository name. */
    repo: string;
    /** Working directory — passed through to backend. */
    cwd: string;
  }

  let { diff, prNumber, owner, repo, cwd }: Props = $props();

  // ── Diff parsing ────────────────────────────────────────────────────────────

  interface DiffLine {
    /** Raw line content (includes the +/-/ prefix). */
    raw: string;
    /** Display content (prefix stripped). */
    content: string;
    /** 'add' | 'del' | 'ctx' | 'hunk' | 'file' */
    kind: 'add' | 'del' | 'ctx' | 'hunk' | 'file';
    /** 1-based position within the unified diff (after the @@ header). */
    position: number | null;
    /** File path this line belongs to (from `+++ b/...` header). */
    filePath: string;
    /** Old line number (for context/delete lines). */
    oldLine: number | null;
    /** New line number (for context/add lines). */
    newLine: number | null;
  }

  function parseDiff(raw: string): DiffLine[] {
    const lines = raw.split('\n');
    const result: DiffLine[] = [];
    let filePath = '';
    let position = 0; // 1-based position counter, reset per hunk group per file
    let oldLine = 0;
    let newLine = 0;
    let inHunk = false;

    for (const line of lines) {
      if (line.startsWith('diff --git') || line.startsWith('index ') ||
          line.startsWith('old mode') || line.startsWith('new mode') ||
          line.startsWith('--- a/') || line.startsWith('Binary ')) {
        inHunk = false;
        result.push({ raw: line, content: line, kind: 'file', position: null, filePath, oldLine: null, newLine: null });
        continue;
      }

      if (line.startsWith('+++ b/')) {
        filePath = line.slice(6);
        position = 0; // reset diff-position counter for this file
        inHunk = false;
        result.push({ raw: line, content: line, kind: 'file', position: null, filePath, oldLine: null, newLine: null });
        continue;
      }

      if (line.startsWith('+++ /dev/null')) {
        filePath = '/dev/null';
        position = 0;
        inHunk = false;
        result.push({ raw: line, content: line, kind: 'file', position: null, filePath, oldLine: null, newLine: null });
        continue;
      }

      if (line.startsWith('@@')) {
        inHunk = true;
        position += 1; // @@ header itself is position N
        // Parse @@ -a,b +c,d @@ to seed line counters
        const m = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
        if (m) {
          oldLine = parseInt(m[1], 10) - 1;
          newLine = parseInt(m[2], 10) - 1;
        }
        result.push({ raw: line, content: line, kind: 'hunk', position, filePath, oldLine: null, newLine: null });
        continue;
      }

      if (!inHunk) {
        result.push({ raw: line, content: line, kind: 'file', position: null, filePath, oldLine: null, newLine: null });
        continue;
      }

      if (line.startsWith('+')) {
        position += 1;
        newLine += 1;
        result.push({ raw: line, content: line.slice(1), kind: 'add', position, filePath, oldLine: null, newLine });
      } else if (line.startsWith('-')) {
        position += 1;
        oldLine += 1;
        result.push({ raw: line, content: line.slice(1), kind: 'del', position, filePath, oldLine, newLine: null });
      } else {
        // Context line (space prefix) or empty (end of hunk)
        position += 1;
        oldLine += 1;
        newLine += 1;
        result.push({ raw: line, content: line.slice(1), kind: 'ctx', position, filePath, oldLine, newLine });
      }
    }

    return result;
  }

  // ── Inline comment state ─────────────────────────────────────────────────────

  interface InlineComment {
    path: string;
    position: number;
    body: string;
  }

  let activeCommentPos = $state<number | null>(null);
  let draftComment = $state('');
  let comments = $state<InlineComment[]>([]);

  function openComment(line: DiffLine) {
    if (line.position === null) return;
    if (activeCommentPos === line.position) {
      // Toggle off
      activeCommentPos = null;
      draftComment = '';
      return;
    }
    activeCommentPos = line.position;
    const existing = comments.find(c => c.position === line.position);
    draftComment = existing?.body ?? '';
  }

  function saveComment(line: DiffLine) {
    if (line.position === null) return;
    const body = draftComment.trim();
    if (!body) {
      comments = comments.filter(c => c.position !== line.position);
    } else {
      const idx = comments.findIndex(c => c.position === line.position);
      if (idx >= 0) {
        comments = comments.map((c, i) => i === idx ? { ...c, body } : c);
      } else {
        comments = [...comments, { path: line.filePath, position: line.position, body }];
      }
    }
    activeCommentPos = null;
    draftComment = '';
  }

  function discardComment() {
    activeCommentPos = null;
    draftComment = '';
  }

  // ── Review submission ────────────────────────────────────────────────────────

  let reviewBody = $state('');
  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let submitSuccess = $state(false);

  async function handleSubmitReview() {
    submitting = true;
    submitError = null;
    submitSuccess = false;
    try {
      const res = await fetch('/api/git/pr/review', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          prNumber,
          owner,
          repo,
          cwd,
          event: 'COMMENT',
          body: reviewBody,
          comments,
        }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`Review submit failed: ${res.status} ${text}`);
      }
      submitSuccess = true;
      reviewBody = '';
      comments = [];
    } catch (e) {
      submitError = e instanceof Error ? e.message : 'Failed to submit review';
    } finally {
      submitting = false;
    }
  }

  // ── Derived ──────────────────────────────────────────────────────────────────

  const parsedLines = $derived(parseDiff(diff));
  const hasDiff = $derived(parsedLines.some(l => l.kind === 'add' || l.kind === 'del'));
</script>

<div class="pr-review" data-testid="pr-review-surface">
  <!-- Header -->
  <header class="review-header">
    <span class="review-title">Review PR #{prNumber}</span>
    <span class="review-repo">{owner}/{repo}</span>
    {#if comments.length > 0}
      <span class="comment-count">{comments.length} comment{comments.length !== 1 ? 's' : ''}</span>
    {/if}
  </header>

  <!-- Success -->
  {#if submitSuccess}
    <div class="banner banner-ok" role="status" data-testid="review-success-banner">
      Review submitted successfully.
    </div>
  {/if}

  <!-- Error -->
  {#if submitError}
    <div class="banner banner-err" role="alert">{submitError}</div>
  {/if}

  <!-- Diff view -->
  <div class="diff-container" data-testid="diff-container">
    {#if !hasDiff && !diff.trim()}
      <div class="empty-diff">No diff available for this PR.</div>
    {:else}
      <table class="diff-table" aria-label="Unified diff">
        <tbody>
          {#each parsedLines as line, idx (idx)}
            {@const isClickable = (line.kind === 'add' || line.kind === 'del') && line.position !== null}
            {@const hasComment = line.position !== null && comments.some(c => c.position === line.position)}
            {@const isActive = line.position !== null && activeCommentPos === line.position}

            {#if line.kind === 'file'}
              <tr class="line-file">
                <td class="gutter gutter-old"></td>
                <td class="gutter gutter-new"></td>
                <td class="line-code file-header" colspan="1">{line.content}</td>
              </tr>
            {:else if line.kind === 'hunk'}
              <tr class="line-hunk">
                <td class="gutter gutter-old"></td>
                <td class="gutter gutter-new"></td>
                <td class="line-code hunk-header">{line.content}</td>
              </tr>
            {:else}
              <tr
                class="line-row line-{line.kind} {isActive ? 'line-active' : ''} {hasComment ? 'has-comment' : ''}"
                onclick={isClickable ? () => openComment(line) : undefined}
                role={isClickable ? 'button' : undefined}
                tabindex={isClickable ? 0 : undefined}
                onkeydown={isClickable ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openComment(line); } } : undefined}
                aria-label={isClickable ? `Line ${line.position}: click to add comment` : undefined}
                aria-pressed={isClickable ? isActive : undefined}
                data-position={line.position}
              >
                <td class="gutter gutter-old">{line.oldLine ?? ''}</td>
                <td class="gutter gutter-new">{line.newLine ?? ''}</td>
                <td class="line-code">
                  <span class="line-prefix" aria-hidden="true">
                    {#if line.kind === 'add'}+{:else if line.kind === 'del'}-{:else} {/if}
                  </span>
                  <span class="line-text">{line.content}</span>
                  {#if hasComment && !isActive}
                    <span class="comment-dot" aria-label="Has comment" title="Has inline comment"></span>
                  {/if}
                </td>
              </tr>

              <!-- Inline comment editor — appears immediately after the target line -->
              {#if isActive}
                <tr class="comment-row">
                  <td class="gutter gutter-old"></td>
                  <td class="gutter gutter-new"></td>
                  <td class="comment-cell">
                    <textarea
                      class="comment-textarea"
                      placeholder="Leave a comment on this line…"
                      aria-label="Inline comment"
                      rows={3}
                      bind:value={draftComment}
                      data-testid="inline-comment-input"
                    ></textarea>
                    <div class="comment-actions">
                      <button
                        class="btn-save-comment"
                        type="button"
                        onclick={() => saveComment(line)}
                        data-testid="save-comment-btn"
                      >Add comment</button>
                      <button
                        class="btn-discard-comment"
                        type="button"
                        onclick={discardComment}
                      >Discard</button>
                    </div>
                  </td>
                </tr>
              {/if}
            {/if}
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <!-- Overall review comment + submit -->
  <div class="review-footer">
    <div class="field-group">
      <label class="field-label" for="review-body">Overall review comment</label>
      <textarea
        id="review-body"
        class="review-textarea"
        placeholder="Summary of your review (optional)"
        aria-label="Overall review body"
        rows={4}
        bind:value={reviewBody}
        data-testid="review-body-textarea"
      ></textarea>
    </div>

    <div class="review-actions">
      <span class="comment-summary">
        {#if comments.length > 0}
          {comments.length} inline comment{comments.length !== 1 ? 's' : ''} queued
        {/if}
      </span>
      <button
        class="submit-btn"
        type="button"
        onclick={handleSubmitReview}
        disabled={submitting}
        aria-disabled={submitting}
        data-testid="submit-review-btn"
      >
        {#if submitting}
          <span class="spinner" aria-hidden="true"></span>
        {/if}
        Submit Review
      </button>
    </div>
  </div>
</div>

<style>
  .pr-review {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--la-bg-panel, #0f1117);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    color: var(--la-text-base, #b4bec8);
    overflow: hidden;
  }

  /* ── Header ─────────────────────────────────────────────────────────────── */
  .review-header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--la-hair-base, #25282d);
    flex-shrink: 0;
  }

  .review-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--la-text-bright, #f1f5f9);
    letter-spacing: 0.06em;
  }

  .review-repo {
    font-size: 11px;
    color: var(--la-text-mute, #5a6472);
  }

  .comment-count {
    margin-left: auto;
    font-size: 11px;
    color: var(--la-struct-primary, #00c8ff);
    background: rgba(0, 200, 255, 0.1);
    padding: 1px 7px;
    border: 1px solid rgba(0, 200, 255, 0.25);
  }

  /* ── Banners ─────────────────────────────────────────────────────────────── */
  .banner {
    padding: 8px 16px;
    font-size: 12px;
    flex-shrink: 0;
  }

  .banner-ok {
    background: rgba(74, 222, 128, 0.1);
    border-bottom: 1px solid var(--la-semantic-ok, #4ade80);
    color: var(--la-semantic-ok, #4ade80);
  }

  .banner-err {
    background: rgba(239, 68, 68, 0.1);
    border-bottom: 1px solid var(--la-semantic-error, #ef4444);
    color: var(--la-semantic-error, #ef4444);
    word-break: break-word;
  }

  /* ── Diff container ──────────────────────────────────────────────────────── */
  .diff-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: auto;
  }

  .empty-diff {
    padding: 24px;
    color: var(--la-text-mute, #5a6472);
    font-size: 12px;
    text-align: center;
  }

  /* ── Diff table ──────────────────────────────────────────────────────────── */
  .diff-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    line-height: 1.5;
  }

  /* Gutters */
  .gutter {
    width: 44px;
    min-width: 44px;
    text-align: right;
    padding: 0 8px;
    color: var(--la-text-mute, #5a6472);
    user-select: none;
    font-size: 11px;
    vertical-align: top;
    border-right: 1px solid var(--la-hair-base, #25282d);
  }

  .gutter-old {
    border-right: none;
  }

  /* Code cell */
  .line-code {
    padding: 0 12px;
    white-space: pre;
    font-family: var(--la-font-mono, monospace);
    vertical-align: top;
    min-width: 0;
    width: 100%;
  }

  /* Line kinds */
  .line-add { background: rgba(74, 222, 128, 0.08); }
  .line-add .line-prefix { color: var(--la-semantic-ok, #4ade80); }
  .line-add .line-text   { color: var(--la-semantic-ok, #4ade80); }
  .line-add .gutter      { background: rgba(74, 222, 128, 0.05); }

  .line-del { background: rgba(239, 68, 68, 0.08); }
  .line-del .line-prefix { color: var(--la-semantic-error, #ef4444); }
  .line-del .line-text   { color: var(--la-semantic-error, #ef4444); }
  .line-del .gutter      { background: rgba(239, 68, 68, 0.05); }

  .line-ctx { background: transparent; }
  .line-ctx .line-prefix { color: var(--la-text-mute, #5a6472); }
  .line-ctx .line-text   { color: var(--la-text-base, #b4bec8); }

  .line-hunk { background: rgba(0, 200, 255, 0.05); }
  .hunk-header { color: var(--la-struct-primary, #00c8ff); font-size: 11px; padding: 2px 12px; }

  .line-file { background: rgba(255, 215, 0, 0.04); }
  .file-header { color: var(--la-accent-gold, #FFD700); font-size: 11px; padding: 4px 12px; }

  /* Clickable lines */
  .line-add,
  .line-del {
    cursor: pointer;
  }

  .line-add:hover,
  .line-del:hover {
    filter: brightness(1.15);
  }

  .line-active {
    outline: 1px solid var(--la-struct-primary, #00c8ff);
    outline-offset: -1px;
  }

  .has-comment .line-code {
    position: relative;
  }

  .comment-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    background: var(--la-struct-primary, #00c8ff);
    border-radius: 50%;
    margin-left: 8px;
    vertical-align: middle;
    flex-shrink: 0;
  }

  .line-prefix {
    display: inline-block;
    width: 14px;
    flex-shrink: 0;
    user-select: none;
  }

  /* ── Inline comment ──────────────────────────────────────────────────────── */
  .comment-row {
    background: rgba(0, 200, 255, 0.04);
    border-top: 1px solid rgba(0, 200, 255, 0.15);
    border-bottom: 1px solid rgba(0, 200, 255, 0.15);
  }

  .comment-cell {
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .comment-textarea {
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    padding: 6px 8px;
    outline: none;
    resize: vertical;
    width: 100%;
    box-sizing: border-box;
    min-height: 60px;
  }

  .comment-textarea:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .comment-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .btn-save-comment {
    padding: 4px 12px;
    background: var(--la-struct-primary, #00c8ff);
    border: none;
    color: var(--la-bg-base, #0a0a0f);
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    letter-spacing: 0.06em;
    transition: opacity 120ms ease;
  }

  .btn-save-comment:hover { opacity: 0.85; }

  .btn-save-comment:focus-visible,
  .btn-discard-comment:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .btn-discard-comment {
    padding: 4px 10px;
    background: none;
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-dim, #96a2ae);
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    cursor: pointer;
    letter-spacing: 0.06em;
    transition: color 120ms ease, border-color 120ms ease;
  }

  .btn-discard-comment:hover {
    color: var(--la-text-bright, #f1f5f9);
    border-color: var(--la-text-dim, #96a2ae);
  }

  /* ── Review footer ───────────────────────────────────────────────────────── */
  .review-footer {
    padding: 12px 16px;
    border-top: 1px solid var(--la-hair-base, #25282d);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--la-bg-panel, #0f1117);
  }

  .field-group {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .field-label {
    font-size: 10px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--la-text-mute, #5a6472);
    user-select: none;
  }

  .review-textarea {
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-base, #25282d);
    color: var(--la-text-bright, #f1f5f9);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    padding: 6px 8px;
    outline: none;
    resize: none;
    width: 100%;
    box-sizing: border-box;
  }

  .review-textarea:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .review-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
  }

  .comment-summary {
    font-size: 11px;
    color: var(--la-text-mute, #5a6472);
    flex: 1;
  }

  .submit-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 7px 18px;
    background: var(--la-struct-primary, #00c8ff);
    border: none;
    color: var(--la-bg-base, #0a0a0f);
    font-family: var(--la-font-mono, monospace);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    cursor: pointer;
    transition: opacity 120ms ease;
    flex-shrink: 0;
  }

  .submit-btn:hover:not(:disabled) { opacity: 0.85; }

  .submit-btn:focus-visible {
    outline: 2px solid var(--la-focus-ring, #FFD700);
    outline-offset: 2px;
  }

  .submit-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* ── Spinner ─────────────────────────────────────────────────────────────── */
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .spinner {
    display: inline-block;
    width: 11px;
    height: 11px;
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
