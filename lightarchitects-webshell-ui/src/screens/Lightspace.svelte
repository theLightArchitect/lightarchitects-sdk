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
  import { ls } from '$lib/lightspace/state.svelte';

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
  import { TimelineEngine } from '$lib/lightspace-timeline';
  import { DEMO_TIMELINE } from '$lib/lightspace-demo-timeline';
  import { canvasClear } from '$lib/lightspace-stores';

  import LightspaceHeader  from '$lib/../components/lightspace/LightspaceHeader.svelte';
  import LeftSidebar       from '$lib/../components/lightspace/LeftSidebar.svelte';
  import BentoCanvas       from '$lib/../components/lightspace/BentoCanvas.svelte';
  import SchematicPanel    from '$lib/../components/lightspace/SchematicPanel.svelte';
  import FileHeroOverlay   from '$lib/../components/lightspace/FileHeroOverlay.svelte';
  import TombHeroOverlay   from '$lib/../components/lightspace/TombHeroOverlay.svelte';
  import Lobby             from './lightspace/Lobby.svelte';

  // @lightarchitects/lightspace-svelte — per-session typed canvas (Wave 3)
  import { subscribeSession } from '@lightarchitects/lightspace-svelte';
  import { authHeaders } from '$lib/auth';

  // Per-session Lightspace UUID. In production, this comes from the server response.
  // In demo mode it is generated client-side so the SSE stream can be exercised locally.
  let lightspaceSessionId = $state<string | null>(null);
  // Conversation session UUID (production mode only) — separate from the canvas session.
  let convSessionId = $state<string | null>(null);

  $effect(() => {
    if (!lightspaceSessionId) return;
    return subscribeSession(lightspaceSessionId, authHeaders);
  });

  import HitlModal from '$lib/../components/ironclaw/HitlModal.svelte';
  import {
    createConversation, sendTurn, subscribeConversation,
  } from '$lib/lightspace/conversation.svelte';

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
  async function handleLobbySubmit(e: CustomEvent<{ intent: string }>) {
    const intent = e.detail.intent.trim();
    if (!intent) return;
    lightspaceSessionStore.update(s => ({
      ...s, intent, runStatus: 'connecting', materializePhase: 'begin',
    }));
    if ($lightspaceSessionStore.mode !== 'production') return;
    try {
      const sessionId = await createConversation(intent);
      await sendTurn(sessionId, intent);
      convSessionId = sessionId;
      lightspaceSessionStore.update(s => ({ ...s, runStatus: 'running' }));
    } catch (err) {
      lightspaceSessionStore.update(s => ({ ...s, runStatus: 'error' }));
      console.error('[conv] lobby submit failed:', err);
    }
  }

  // ── Phase 6: Demo TIMELINE engine ────────────────────────────────────────
  let timeline: TimelineEngine | null = null;

  $effect(() => {
    const mode = $lightspaceSessionStore.mode;
    if (mode === 'demo' && !lightspaceSessionId) {
      lightspaceSessionId = crypto.randomUUID();
    }
  });

  $effect(() => {
    const mode = $lightspaceSessionStore.mode;
    if (mode === 'demo') {
      canvasClear();
      timeline = new TimelineEngine(DEMO_TIMELINE);
      timeline.play();
    } else {
      timeline?.pause();
      timeline = null;
    }
    return () => { timeline?.pause(); timeline = null; };
  });

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

  // ── Conversation SSE subscription (production mode only) ─────────────────
  $effect(() => {
    const sessionId = convSessionId;
    const mode = $lightspaceSessionStore.mode;
    if (!sessionId || mode !== 'production') return;
    return subscribeConversation(
      sessionId,
      (ev) => {
        switch (ev.type) {
          case 'activity':
            sessionAddConvMessage({ id: crypto.randomUUID(), who: 'copilot', text: ev.summary ?? '', ts: Date.now() });
            break;
          case 'strategy_phase':
            canvasUpsertCard({
              id: 'strategy-phase', kind: 'branchlane', span: 'span-12',
              title: `Phase: ${ev.phase}`, ts: Date.now(),
              data: { phase: ev.phase, strategy: ev.strategy },
            });
            break;
          case 'hitl_pause':
            ironclawHitlEscalation.set({
              type: 'ironclaw_hitl_escalation',
              build_id: $lightspaceSessionStore.buildId ?? sessionId,
              task_id: sessionId,
              decision_topic: 'conversation_hitl',
              layer_failed: 0,
              escalation_question: ev.prompt ?? '',
              nonce: ev.nonce ?? '',
            });
            break;
          case 'done':
            lightspaceSessionStore.update(s => ({ ...s, runStatus: 'complete' }));
            break;
          case 'error':
            lightspaceSessionStore.update(s => ({ ...s, runStatus: 'error' }));
            break;
          case 'lag':
            console.warn(`[conv-sse] lag: ${ev.skipped ?? 0} events dropped`);
            break;
        }
      },
      (errMsg) => {
        console.error('[conv-sse] stream error:', errMsg);
        lightspaceSessionStore.update(s => ({ ...s, runStatus: 'error' }));
      },
    );
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
    // Restore session from localStorage — allows page refresh without losing session.
    // Direct field assignment skips the materialize animation (no SSE events will fire it).
    const savedSessionId = localStorage.getItem('la_ls_session_id');
    if (savedSessionId) {
      ls.sessionId = savedSessionId;
      ls.inLobby = false;
      ls.materializing = false;
      ls.wsState = 'materialised';
    }

    const handler = (e: Event) => { void handleLobbySubmit(e as CustomEvent<{ intent: string }>); };
    document.addEventListener('ls:lobby-submit', handler);
    return () => document.removeEventListener('ls:lobby-submit', handler);
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

  {#if $ironclawHitlEscalation}<HitlModal />{/if}

  {#if ls.inLobby}<Lobby />{/if}

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
