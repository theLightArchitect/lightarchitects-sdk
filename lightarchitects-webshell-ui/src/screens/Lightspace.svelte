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
   * Phase 4: all structural components mounted; SSE subscription wired in Phase 5;
   * TIMELINE demo engine wired in Phase 6.
   */

  import '../styles/lightspace-tokens.css';
  import { page } from '$app/state';
  import { onMount } from 'svelte';

  import {
    lightspaceSessionStore,
    lightspaceFilesStore,
    lightspaceCanvasStore,
    lightspaceUiStore,
  } from '$lib/lightspace-stores';

  import LightspaceHeader  from '$lib/../components/lightspace/LightspaceHeader.svelte';
  import LeftSidebar       from '$lib/../components/lightspace/LeftSidebar.svelte';
  import BentoCanvas       from '$lib/../components/lightspace/BentoCanvas.svelte';
  import SchematicPanel    from '$lib/../components/lightspace/SchematicPanel.svelte';
  import FileHeroOverlay   from '$lib/../components/lightspace/FileHeroOverlay.svelte';
  import TombHeroOverlay   from '$lib/../components/lightspace/TombHeroOverlay.svelte';

  // ── HITL modal (Phase 4) — reuse existing ironclaw HitlModal ────────────
  // Phase 5 wires the ironclawHitlEscalation store; placeholder here.
  // import HitlModal from '$lib/../components/ironclaw/HitlModal.svelte';

  // ── Route context ─────────────────────────────────────────────────────────
  const buildId = $derived(page.params.buildId ?? null);

  $effect(() => {
    lightspaceSessionStore.update(s => ({ ...s, buildId }));
  });

  // ── Keyboard: Esc to dismiss overlays / collapse expanded card ─────────────
  function handleKeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    // Priority: file hero → tomb hero → expanded card
    if ($lightspaceFilesStore.heroFileId) {
      lightspaceFilesStore.update(s => ({ ...s, heroFileId: null }));
      return;
    }
    if ($lightspaceFilesStore.heroTombId) {
      lightspaceFilesStore.update(s => ({ ...s, heroTombId: null }));
      return;
    }
    if ($lightspaceCanvasStore.expandedCardId) {
      lightspaceCanvasStore.update(s => ({ ...s, expandedCardId: null }));
    }
  }

  // ── Lobby submit handler (dispatched by LobbyInput) ───────────────────────
  function handleLobbySubmit(e: CustomEvent<{ intent: string }>) {
    const intent = e.detail.intent.trim();
    if (!intent) return;
    lightspaceSessionStore.update(s => ({
      ...s, intent, runStatus: 'connecting', materializePhase: 'begin',
    }));
    // Phase 5: wire to POST /api/lightshell/runs in production mode
    // Phase 6: trigger TIMELINE engine in demo mode
  }

  onMount(() => {
    document.addEventListener('ls:lobby-submit', handleLobbySubmit as EventListener);
    return () => document.removeEventListener('ls:lobby-submit', handleLobbySubmit as EventListener);
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="ls-root" data-mode={$lightspaceSessionStore.mode}>

  <!-- Header -->
  <LightspaceHeader />

  <!-- 3-panel workspace -->
  <div class="ls-workspace">
    <LeftSidebar />
    <BentoCanvas />
    <SchematicPanel />
  </div>

  <!-- Overlays (rendered on top of workspace) -->
  <FileHeroOverlay />
  <TombHeroOverlay />

  <!-- Phase 5: HitlModal (ironclawHitlEscalation store wired) -->
  <!-- Phase 6: LightspaceTimeline (demo TIMELINE engine) -->

</div>

<style>
.ls-root {
  width: 100%;
  height: 100%;
  position: relative;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.ls-workspace {
  flex: 1;
  display: flex;
  min-height: 0;
  overflow: hidden;
}
</style>
