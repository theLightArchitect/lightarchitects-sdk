<script lang="ts">
  /**
   * @component Lightspace
   * @description Root Lightspace screen — d0 entry surface. Mounts the full
   *   bento-grid workspace from lobby through to live card streams.
   *
   * @contract EventType multiple → activityFeed, implCompleteEvents, mergeAgentEvents,
   *   ironclawHitlEscalation, workerSlots, conductorState, pillarStream (all via sse.ts)
   * @reads  lightspaceSessionStore, lightspaceCanvasStore, lightspaceFilesStore,
   *         lightspaceUiStore, lightspaceLasdlcStore, lightspaceMetricsStore
   * @mutates lightspaceCanvasStore (canvasAddCard on SSE), lightspaceSessionStore
   * @api GET /api/builds/:buildId (production mode init), GET /api/builds/:id/fleet
   *
   * Phase 1: root shell + store imports. BentoCanvas, LeftSidebar, SchematicPanel
   * and overlays are wired in Phases 2–4. SSE subscriptions wired in Phase 5.
   * Demo TIMELINE engine wired in Phase 6.
   */

  import '../styles/lightspace-tokens.css';
  import { page } from '$app/state';

  import {
    lightspaceSessionStore,
    lightspaceCanvasStore,
    lightspaceFilesStore,
    lightspaceUiStore,
    lightspaceLasdlcStore,
    lightspaceMetricsStore,
  } from '$lib/lightspace-stores';

  // ── Route context ─────────────────────────────────────────────────────────
  // SvelteKit provides buildId via page.params when mounted from
  // src/routes/lightspace/[buildId]/+page.svelte.
  const buildId = $derived(page.params.buildId ?? null);

  // Set buildId in session store when route param changes.
  $effect(() => {
    lightspaceSessionStore.update(s => ({ ...s, buildId }));
  });
</script>

<!-- Root shell — CSS class is .ls-root (design tokens scoped here) -->
<div class="ls-root" data-testid="lightspace-root" data-mode={$lightspaceSessionStore.mode}>

  <!-- Phase 2: BentoCanvas renders here -->
  <!-- Phase 3: LeftSidebar + SchematicPanel render here -->
  <!-- Phase 4: LightspaceHeader + overlays render here -->
  <!-- Phase 5: SSE store subscriptions wired in script -->
  <!-- Phase 6: TIMELINE demo engine wired in script -->

  <!-- Placeholder until Phase 2 -->
  <div class="ls-placeholder">
    <span class="ls-placeholder-glyph">◇</span>
    <span>Lightspace initialising…</span>
  </div>

</div>

<style>
  .ls-root {
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .ls-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 12px;
    color: var(--ls-text-ghost, rgba(255,255,255,0.12));
    font-family: var(--ls-font-code, ui-monospace, monospace);
    font-size: 11px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .ls-placeholder-glyph {
    font-size: 28px;
    font-family: var(--ls-font-display, sans-serif);
    font-weight: 800;
  }
</style>
