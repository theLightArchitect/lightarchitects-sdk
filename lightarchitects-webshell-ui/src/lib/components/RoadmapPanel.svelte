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
  // ADD_TAGS:['style'] preserves roadmap internal CSS; ALLOWED_ATTR:[] whitelist
  // denies all event handlers (whitelist beats blacklist per Security Guardrails §3.2 C5).
  let sanitizedHtml = $derived(
    $roadmapStore.status === 'success'
      ? DOMPurify.sanitize($roadmapStore.rawHtml, {
          ADD_TAGS: ['style'],
          ALLOWED_ATTR: [],
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
      <!-- Loading skeleton — staggered scan rows, no blank screen on first paint -->
      <div class="skeleton" aria-hidden="true" aria-label="Loading roadmap content">
        <div class="sk-group">
          <div class="sk-row sk-wide"></div>
          <div class="sk-row sk-med"></div>
          <div class="sk-row sk-narrow"></div>
        </div>
        <div class="sk-group">
          <div class="sk-row sk-wide"></div>
          <div class="sk-row sk-med"></div>
          <div class="sk-row sk-wide"></div>
          <div class="sk-row sk-narrow"></div>
        </div>
        <div class="sk-group">
          <div class="sk-row sk-med"></div>
          <div class="sk-row sk-wide"></div>
        </div>
      </div>

    {:else if status === 'error'}
      <!-- Error banner with pulsing indicator and retry -->
      <div class="error-banner" role="alert">
        <span class="err-pulse-dot" aria-hidden="true"></span>
        <span class="error-icon" aria-hidden="true">⚠</span>
        <span class="error-msg">{$roadmapStore.error}</span>
        <button class="retry-btn" onclick={() => roadmapStore.fetch()}>
          RETRY
        </button>
      </div>

    {:else if status === 'empty'}
      <!-- Empty state — no roadmap.html artifact yet -->
      <div class="empty-state" role="status">
        <span class="empty-glyph" aria-hidden="true">◈</span>
        <p class="empty-label" aria-hidden="true">NO ARTIFACT</p>
        <p class="empty-text">No roadmap artifact available</p>
        <p class="empty-hint">/SYNC --roadmap to generate content</p>
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
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    /* Subtle dot-grid gives skeleton the feel of graph paper */
    background-image:
      radial-gradient(circle, rgba(0,200,255,0.04) 1px, transparent 1px);
    background-size: 20px 20px;
    background-position: -1px -1px;
  }

  .sk-group {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .sk-row {
    height: 10px;
    border-radius: 1px;
    background: linear-gradient(
      90deg,
      var(--la-bg-elevated, #1a2030) 0%,
      rgba(0, 200, 255, 0.10) 45%,
      rgba(0, 200, 255, 0.03) 55%,
      var(--la-bg-elevated, #1a2030) 100%
    );
    background-size: 250% 100%;
    animation: sk-scan 2.4s ease-in-out infinite;
  }

  /* Staggered delays — waterfall scan illusion, reads as live data arriving */
  .sk-group:nth-child(1) .sk-row:nth-child(1) { animation-delay:   0ms; }
  .sk-group:nth-child(1) .sk-row:nth-child(2) { animation-delay: 100ms; }
  .sk-group:nth-child(1) .sk-row:nth-child(3) { animation-delay: 200ms; }
  .sk-group:nth-child(2) .sk-row:nth-child(1) { animation-delay: 320ms; }
  .sk-group:nth-child(2) .sk-row:nth-child(2) { animation-delay: 420ms; }
  .sk-group:nth-child(2) .sk-row:nth-child(3) { animation-delay: 520ms; }
  .sk-group:nth-child(2) .sk-row:nth-child(4) { animation-delay: 620ms; }
  .sk-group:nth-child(3) .sk-row:nth-child(1) { animation-delay: 740ms; }
  .sk-group:nth-child(3) .sk-row:nth-child(2) { animation-delay: 840ms; }

  .sk-wide   { width: 100%; }
  .sk-med    { width: 64%; }
  .sk-narrow { width: 39%; }

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
    padding: 10px 16px;
    margin: 10px 12px;
    border: 1px solid rgba(239, 68, 68, 0.20);
    border-left: 3px solid var(--la-semantic-error, #ef4444);
    background: rgba(239, 68, 68, 0.04);
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    flex-shrink: 0;
  }

  /* Pulsing warning dot — operator-grade alert indicator */
  .err-pulse-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--la-semantic-error, #ef4444);
    flex-shrink: 0;
    animation: err-pulse 1.6s ease-in-out infinite;
  }

  @keyframes err-pulse {
    0%, 100% { opacity: 1; box-shadow: 0 0 0 0 rgba(239,68,68,0.4); }
    50%       { opacity: 0.45; box-shadow: 0 0 0 4px rgba(239,68,68,0); }
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
    letter-spacing: 0.10em;
    padding: 3px 8px;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--la-transition-fast, 120ms ease),
                box-shadow var(--la-transition-fast, 120ms ease);
  }

  .retry-btn:hover {
    background: rgba(0, 200, 255, 0.08);
    box-shadow: 0 0 6px rgba(0, 200, 255, 0.15);
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
    gap: 6px;
    padding: 32px;
    text-align: center;
    position: relative;
    overflow: hidden;
  }

  /* Concentric rings — mission control "no signal" radar */
  .empty-state::before {
    content: '';
    position: absolute;
    width: 280px;
    height: 280px;
    border-radius: 50%;
    border: 1px solid rgba(0, 200, 255, 0.05);
    box-shadow:
      0 0 0 50px rgba(0, 200, 255, 0.025),
      0 0 0 100px rgba(0, 200, 255, 0.01);
    pointer-events: none;
  }

  /* Single geometric glyph — diamond/compass marker */
  .empty-glyph {
    display: block;
    font-family: var(--la-font-mono, monospace);
    font-size: 26px;
    color: rgba(0, 200, 255, 0.18);
    margin-bottom: 10px;
    position: relative;
    animation: empty-breathe 3.5s ease-in-out infinite;
  }

  @keyframes empty-breathe {
    0%, 100% { opacity: 0.6; }
    50%       { opacity: 1; }
  }

  .empty-label {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.30em;
    color: rgba(0, 200, 255, 0.30);
    text-transform: uppercase;
    margin: 0 0 4px;
    position: relative;
  }

  .empty-text {
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    font-weight: 500;
    color: var(--la-text-dim, #96a2ae);
    position: relative;
    margin: 0;
  }

  .empty-hint {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-mute, #5a6472);
    letter-spacing: 0.04em;
    position: relative;
    margin: 0;
  }

  /* ── Content host ─────────────────────────────────────────────────────── */
  .content-host {
    flex: 1;
    overflow: auto;
    position: relative;
    /*
     * Traps position:fixed children from the injected roadmap HTML
     * (sticky header, progress bar, particle canvas) within this
     * scroll container. Without this, they escape and overlay the webshell nav.
     * CSS containment spec: any ancestor with transform/perspective/filter
     * becomes the containing block for position:fixed descendants.
     */
    transform: translate(0, 0);
  }

  /* Left-edge sweep on SSE-driven refresh — reads as a data pulse arriving */
  .content-host::before {
    content: '';
    position: absolute;
    inset: 0;
    left: 0;
    width: 2px;
    background: var(--la-struct-primary, #00c8ff);
    opacity: 0;
    pointer-events: none;
    z-index: 10;
  }

  .content-host.flash::before {
    animation: edge-sweep 300ms ease-out forwards;
  }

  @keyframes edge-sweep {
    0%   { opacity: 0.9; top: 0; height: 0; }
    40%  { opacity: 0.9; top: 0; height: 100%; }
    100% { opacity: 0;   top: 0; height: 100%; }
  }
</style>
