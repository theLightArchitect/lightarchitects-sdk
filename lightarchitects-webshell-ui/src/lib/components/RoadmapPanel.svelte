<script lang="ts">
  /**
   * RoadmapPanel — /#/roadmap tab (portfolio-zoom level, P6 Northstar).
   *
   * Fetches /api/roadmap (static roadmap.html artifact, ≤256KB, DOMPurify-sanitized).
   * Auto-refreshes on `la:build-update` DOM events dispatched by sse.ts.
   *
   * State machine: idle → loading → success | error | empty
   *   success + la:build-update → loading → success | error
   *   error + retry click       → loading → success | error
   *
   * SSE cleanup: onMount returns teardown fn (removeEventListener).
   * Tab router uses {#if} not CSS display:none — onMount/cleanup fire on tab switch.
   *
   * Security invariants (plan §5.4 + §10):
   *   - DOMPurify.sanitize() with ADD_TAGS:['style'], FORBID_ATTR event-handler list
   *   - No innerHTML assignment without prior sanitization
   *   - transform:translate(0,0) on .content-host traps position:fixed children
   *     from roadmap HTML (header, progress bar, canvas) within panel scroll container
   */

  import { onMount } from 'svelte';
  import DOMPurify from 'dompurify';
  import { roadmapStore } from '$lib/roadmap';
  import SectionHeader from '$lib/components/SectionHeader.svelte';

  // ── Local reactive state ───────────────────────────────────────────────────

  /** Flash true for 150ms on BuildUpdate arrival to signal live refresh. */
  let refreshFlash = $state(false);

  // ── Derived from store ─────────────────────────────────────────────────────

  let status = $derived($roadmapStore.status);

  // Sanitized HTML ready for {@html} injection.
  // ADD_TAGS:['style'] preserves roadmap internal CSS; scripts always stripped.
  let sanitizedHtml = $derived(
    $roadmapStore.status === 'success'
      ? DOMPurify.sanitize($roadmapStore.rawHtml, {
          ADD_TAGS: ['style'],
          FORBID_ATTR: [
            'onclick', 'onmouseover', 'onmouseenter', 'onmouseleave',
            'onerror', 'onload', 'onunload', 'onfocus', 'onblur',
            'onkeydown', 'onkeyup', 'onkeypress', 'onsubmit',
          ],
        })
      : ''
  );

  let lastUpdatedStr = $derived(
    $roadmapStore.lastUpdated
      ? new Date($roadmapStore.lastUpdated).toLocaleTimeString('en', {
          hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit',
        })
      : ''
  );

  // ── SSE → refetch ──────────────────────────────────────────────────────────

  function onBuildUpdate() {
    if ($roadmapStore.status === 'success') {
      refreshFlash = true;
      setTimeout(() => { refreshFlash = false; }, 150);
    }
    roadmapStore.fetch();
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  onMount(() => {
    roadmapStore.fetch();
    window.addEventListener('la:build-update', onBuildUpdate);
    // Return cleanup — fires on tab switch ({#if} unmount) and component destroy.
    return () => window.removeEventListener('la:build-update', onBuildUpdate);
  });
</script>

<div class="roadmap-panel" role="region" aria-label="Roadmap">

  <!-- ── Header ─────────────────────────────────────────────────────────── -->
  <SectionHeader number="R" label="Roadmap" metadata={lastUpdatedStr || undefined}>
    {#if status === 'loading'}
      <span class="hdr-spinner" aria-label="Loading roadmap" role="status">
        <span class="spinner"></span>
      </span>
    {/if}
  </SectionHeader>

  <!-- ── Body ───────────────────────────────────────────────────────────── -->
  <div class="roadmap-body">

    {#if status === 'idle' || status === 'loading'}
      <!-- Loading skeleton — 3 scan-line rows, no blank screen on first paint -->
      <div class="skeleton" aria-hidden="true" aria-label="Loading roadmap content">
        <div class="sk-row sk-wide"></div>
        <div class="sk-row sk-med"></div>
        <div class="sk-row sk-narrow"></div>
        <div class="sk-row sk-wide" style="margin-top: 8px;"></div>
        <div class="sk-row sk-med"></div>
      </div>

    {:else if status === 'error'}
      <!-- Error banner with retry -->
      <div class="error-banner" role="alert">
        <span class="error-icon" aria-hidden="true">⚠</span>
        <span class="error-msg">{$roadmapStore.error}</span>
        <button class="retry-btn" onclick={() => roadmapStore.fetch()}>
          RETRY
        </button>
      </div>

    {:else if status === 'empty'}
      <!-- Empty state — roadmap.html exists but is empty -->
      <div class="empty-state" role="status">
        <span class="empty-glyph" aria-hidden="true">[ — ]</span>
        <p class="empty-text">No roadmap artifact available</p>
        <p class="empty-hint">Run /SYNC --roadmap to generate content</p>
      </div>

    {:else}
      <!-- Success: DOMPurify-sanitized roadmap HTML -->
      <!--
        transform:translate(0,0) creates a new containing block for position:fixed
        children inside the roadmap HTML (header, progress bar, particle canvas).
        Without this, they escape the scroll container and overlay the webshell nav.
      -->
      <div
        class="content-host"
        class:flash={refreshFlash}
        aria-label="Roadmap content"
      >
        {@html sanitizedHtml}
      </div>
    {/if}

  </div>
</div>

<style>
  /* ── Panel shell ────────────────────────────────────────────────────────── */
  .roadmap-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--la-bg-panel, #0f1117);
    overflow: hidden;
  }

  .roadmap-body {
    flex: 1;
    overflow: hidden;
    position: relative;
    display: flex;
    flex-direction: column;
  }

  /* ── Loading skeleton ─────────────────────────────────────────────────── */
  .skeleton {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .sk-row {
    height: 11px;
    background: linear-gradient(
      90deg,
      var(--la-bg-elevated, #1a2030) 25%,
      rgba(0, 200, 255, 0.07) 50%,
      var(--la-bg-elevated, #1a2030) 75%
    );
    background-size: 300% 100%;
    animation: sk-scan 1.8s ease-in-out infinite;
  }

  /* Varying widths create the "table of data" illusion */
  .sk-wide   { width: 100%; }
  .sk-med    { width: 67%; }
  .sk-narrow { width: 42%; }

  @keyframes sk-scan {
    0%   { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }

  /* ── Spinner in SectionHeader right slot ─────────────────────────────── */
  .hdr-spinner {
    display: flex;
    align-items: center;
    padding-right: 2px;
  }

  .spinner {
    display: inline-block;
    width: 8px;
    height: 8px;
    border: 1.5px solid var(--la-struct-primary, #00c8ff);
    border-top-color: transparent;
    border-radius: 50%;
    animation: spin 700ms linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* ── Error banner ─────────────────────────────────────────────────────── */
  .error-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    margin: 0;
    border-left: 2px solid var(--la-semantic-error, #ef4444);
    background: rgba(239, 68, 68, 0.05);
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    flex-shrink: 0;
  }

  .error-icon {
    color: var(--la-semantic-error, #ef4444);
    font-size: 11px;
    flex-shrink: 0;
  }

  .error-msg {
    flex: 1;
    color: var(--la-text-dim, #96a2ae);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .retry-btn {
    background: transparent;
    border: 1px solid var(--la-struct-primary, #00c8ff);
    color: var(--la-struct-primary, #00c8ff);
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 3px 8px;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--la-transition-fast, 120ms ease);
  }

  .retry-btn:hover {
    background: rgba(0, 200, 255, 0.08);
  }

  .retry-btn:active {
    background: rgba(0, 200, 255, 0.16);
  }

  /* ── Empty state ──────────────────────────────────────────────────────── */
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px;
    text-align: center;
  }

  .empty-glyph {
    display: block;
    font-family: var(--la-font-mono, monospace);
    font-size: 22px;
    font-weight: 200;
    color: var(--la-text-mute, #5a6472);
    letter-spacing: 0.15em;
    margin-bottom: 4px;
  }

  .empty-text {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    font-weight: 500;
    color: var(--la-text-dim, #96a2ae);
  }

  .empty-hint {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-mute, #5a6472);
    letter-spacing: 0.05em;
  }

  /* ── Content host ─────────────────────────────────────────────────────── */
  .content-host {
    flex: 1;
    overflow: auto;
    /*
     * Traps position:fixed children from the injected roadmap HTML
     * (sticky header, progress bar, particle canvas) within this
     * scroll container. Without this, they escape and overlay the webshell nav.
     * CSS containment spec: any ancestor with transform/perspective/filter
     * becomes the containing block for position:fixed descendants.
     */
    transform: translate(0, 0);
    border-left: 2px solid transparent;
    transition: border-left-color 150ms ease;
  }

  /* 150ms cyan flash on SSE-driven refresh — tactile signal of live update */
  .content-host.flash {
    border-left-color: var(--la-struct-primary, #00c8ff);
  }
</style>
