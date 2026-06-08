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
    canvasAddCard,
    canvasUpsertCard,
    lasdlcUpdateGates,
    sessionAddConvMessage,
  } from '$lib/lightspace-stores';
  import { activityFeed, implCompleteEvents, mergeAgentEvents, ironclawHitlEscalation } from '$lib/stores';
  import { subscribeFleet } from '$lib/sse';
  import type { FleetEvent } from '$lib/types';

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

  // ── Phase 5: SSE store subscriptions ─────────────────────────────────────
  // sse.ts already populates all these stores. Lightspace subscribes reactively
  // and maps new events to canvas mutations. Track array lengths to process
  // only newly-appended events (avoids re-processing full history on each tick).

  let lastActivityLen = 0;
  $effect(() => {
    const feed = $activityFeed;
    const newItems = feed.slice(lastActivityLen);
    lastActivityLen = feed.length;
    for (const entry of newItems) {
      // activityFeed has 3 sources: 'copilot' | 'ayin' | 'supervisor'
      // ayin spans → AYIN observability only; supervisor alerts → supervisorAlerts store
      if (entry.source !== 'copilot') continue;
      const ev = entry.event;
      const id = `trace-${ev.timestamp}-${Math.random().toString(36).slice(2,7)}`;
      switch (ev.kind) {
        case 'thinking':
          canvasAddCard({ id, kind: 'thinking', span: 'span-6', title: 'Reasoning', ts: Date.now(), data: { summary: ev.summary ?? '', full: ev.summary ?? '' } });
          break;
        case 'tool_use': {
          const isBash = (ev.raw as Record<string, unknown>)?.name?.toString().startsWith('Bash');
          if (isBash) {
            canvasAddCard({ id, kind: 'bash', span: 'span-4', title: 'Bash', ts: Date.now(), data: { output: ev.summary ?? '' } });
          } else {
            canvasAddCard({ id, kind: 'toolcall', span: 'span-4', title: ev.summary?.slice(0, 40) ?? 'tool', ts: Date.now(), data: { name: (ev.raw as Record<string, unknown>)?.name ?? 'tool_use', args: ev.summary ?? '' } });
          }
          break;
        }
        default:
          canvasUpsertCard({ id: `trace-${$lightspaceSessionStore.buildId ?? 'demo'}`, kind: 'trace', span: 'span-6', title: 'Activity', ts: Date.now(), data: { entries: [{ kind: ev.kind ?? 'assistant', text: ev.summary ?? '' }] } });
      }
    }
  });

  let lastImplLen = 0;
  $effect(() => {
    const events = $implCompleteEvents;
    const newItems = events.slice(lastImplLen);
    lastImplLen = events.length;
    for (const ev of newItems) {
      const base = { ts: Date.now(), data: ev };
      canvasAddCard({ id: `diff-${ev.commit_sha?.slice(0,8) ?? Math.random().toString(36).slice(2)}`, kind: 'diff', span: 'span-12', title: `Wave ${ev.wave ?? ''} diff`, ...base });
      canvasAddCard({ id: `art-${ev.commit_sha?.slice(0,8) ?? Math.random().toString(36).slice(2)}`, kind: 'artifact', span: 'span-3', title: ev.task_id ?? 'artifact', ...base });
      lasdlcUpdateGates(ev.gates_passed ?? [], ev.gates_skipped ?? []);
    }
  });

  let lastMergeLen = 0;
  $effect(() => {
    const events = $mergeAgentEvents;
    const newItems = events.slice(lastMergeLen);
    lastMergeLen = events.length;
    for (const ev of newItems) {
      canvasUpsertCard({ id: `bl-${$lightspaceSessionStore.buildId ?? 'demo'}`, kind: 'branchlane', span: 'span-12', title: 'Phase Ladder', ts: Date.now(), data: { lanes: [] } });
    }
  });

  $effect(() => {
    if ($ironclawHitlEscalation) {
      // Phase 4.5: HitlModal is imported in Phase 5 full integration.
      // For now just log — HitlModal will open reactively via ironclawHitlEscalation store.
    }
  });

  // Fleet subscription (production mode only)
  let cleanupFleet: (() => void) | null = null;
  $effect(() => {
    const id = $lightspaceSessionStore.buildId;
    if (!id || $lightspaceSessionStore.mode !== 'production') {
      cleanupFleet?.();
      cleanupFleet = null;
      return;
    }
    cleanupFleet?.();
    cleanupFleet = subscribeFleet(id, (ev: FleetEvent) => {
      if (ev.type === 'snapshot') {
        for (const node of ev.nodes) {
          canvasUpsertCard({ id: `agent-${node.agent_id}`, kind: 'agentspawn', span: 'span-4', title: node.agent_type, ts: Date.now(), data: { agentType: node.agent_type, status: node.status, progress: 0 } });
        }
      } else if (ev.type === 'agent_spawned') {
        canvasAddCard({ id: `agent-${ev.node.agent_id}`, kind: 'agentspawn', span: 'span-4', title: ev.node.agent_type, ts: Date.now(), data: { agentType: ev.node.agent_type, status: 'running', progress: 0 } });
      } else if (ev.type === 'agent_completed') {
        canvasUpsertCard({ id: `agent-${ev.agent_id}`, _agentDone: true });
      }
    });
    return () => { cleanupFleet?.(); cleanupFleet = null; };
  });

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
