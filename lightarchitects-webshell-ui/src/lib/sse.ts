// ============================================================================
// SSE client — connect to per-build /api/builds/:id/events stream
// Uses fetch() instead of EventSource to support auth headers.
// ============================================================================

import type { EventType, Pillar, FleetEvent } from './types';
import { authHeaders } from './auth';
import {
  ayinStatus, authStatus, siblingHealth, waves, builds, findings,
  conductorTasks, arenaStatus, alerts, selectedPillar,
  copilotMessages, copilotLoading, buildFocusActive,
  helixEntries, promotionFeed, hotMemory, coldMemory,
  appendPillarUpdate, appendActivity, activityActive,
  appendSupervisorAlert,
  tagBuildAccess,
  activePlan, updatePlanPhase,
  latestScrumReport,
  trainingRun,
  mailboxMessages, mailboxUnread,
  contextUsage,
  gitforestTree, gitforestPulses,
  workerSlots, conductorState, mergeAgentEvents, fixAgentEvents,
  pushRecentEvent,
} from './stores';
import { spikeSibling } from './stores';
import { reconstructTopology } from './gitforest';
import { invalidate as invalidateGitForestCache } from './gitforestCache';
import { get, writable } from 'svelte/store';
import type {
  SiblingId, Build, Finding, ConductorTask, ArenaAgent,
  HelixEntrySsePayload, SoulPromotionPayload, ContextMemo,
  PillarUpdatePayload, CopilotActivityEvent, AyinSpanEvent, ContextStatusEvent,
  SupervisorAlert, SupervisorGate, SupervisorVerdict,
  ActivePlan, PlanPhaseStatus,
  ScrumReport, ScrumFinding,
  TrainingRun, TrainingRunStatus,
} from './types';

/** Gate-action keywords that identify supervisor decisions in AYIN spans. */
const SUPERVISOR_GATE_ACTIONS = new Set<string>(['guard', 'alpha', 'quality', 'canon']);

/**
 * Extract a supervisor decision from an AYIN span, if it represents one.
 * Returns null if the span is not a supervisor gate event.
 */
function extractSupervisorAlert(span: AyinSpanEvent): SupervisorAlert | null {
  // Only CORSO spans can be supervisor decisions
  if (span.actor !== 'corso') return null;

  // Check if the action matches a known gate
  const actionLower = (span.action ?? '').toLowerCase();
  let gate: SupervisorGate | null = null;
  for (const g of SUPERVISOR_GATE_ACTIONS) {
    if (actionLower === g || actionLower.startsWith(g + '_') || actionLower.startsWith(g + ':') || actionLower.includes('_' + g)) {
      gate = g as SupervisorGate;
      break;
    }
  }
  if (!gate) return null;

  // Determine verdict from outcome
  const outcome = span.outcome;
  let verdict: SupervisorVerdict = 'PASS';
  let message = '';
  let details: string | undefined;

  if (typeof outcome === 'string') {
    const lower = outcome.toLowerCase();
    if (lower.includes('fail') || lower.includes('block') || lower.includes('reject') || lower.includes('denied')) {
      verdict = 'FAIL';
    } else if (lower.includes('warn')) {
      verdict = 'WARN';
    }
    message = outcome;
  } else if (outcome && typeof outcome === 'object') {
    const obj = outcome as Record<string, unknown>;
    const status = String(obj.status ?? obj.verdict ?? obj.result ?? '').toLowerCase();
    if (status.includes('fail') || status.includes('block') || status.includes('reject') || status.includes('denied')) {
      verdict = 'FAIL';
    } else if (status.includes('warn')) {
      verdict = 'WARN';
    }
    message = String(obj.message ?? obj.summary ?? obj.reason ?? `${gate.toUpperCase()} gate ${verdict}`).slice(0, 500);
    details = obj.details ? String(obj.details).slice(0, 4096) : undefined;
  }

  if (!message) {
    message = `CORSO ${gate.toUpperCase()}: ${verdict}`;
  }

  return {
    id: `sv-${span.id}`,
    timestamp: new Date(span.timestamp).getTime() || Date.now(),
    sibling: span.actor,
    gate,
    verdict,
    message,
    details,
  };
}

/** Maximum helix_entry events retained in the rolling window store. */
const HELIX_ENTRIES_WINDOW = 500;

/**
 * Whether the SSE connection is currently established.
 *
 * Set to `true` when the fetch response is OK and the stream loop starts.
 * Set to `false` on stream end, error, or abort. Consumers can subscribe
 * to render a retry button when the connection drops.
 */
export const sseConnected = writable<boolean>(false);

const MAX_BACKOFF = 30_000;
const INITIAL_DELAY = 1_000;

let abortController: AbortController | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let currentDelay = INITIAL_DELAY;
let currentBuildId: string | null = null;

export function connectSSE(buildId: string): void {
  currentBuildId = buildId;
  disconnectSSE();
  currentDelay = INITIAL_DELAY;
  _connect();
}

function _connect(): void {
  if (!currentBuildId) return;
  abortController = new AbortController();
  const { signal } = abortController;

  fetch(`/api/builds/${currentBuildId}/events`, { signal, headers: authHeaders() })
    .then(async (response) => {
      if (!response.ok || !response.body) {
        console.error(`SSE: ${response.status} ${response.statusText}`);
        if (response.status === 401) {
          authStatus.set('unauthorized');
          ayinStatus.set('offline');
          return; // not transient — do not retry until token changes
        }
        if (response.status === 403) {
          authStatus.set('forbidden');
          ayinStatus.set('offline');
          return;
        }
        _scheduleReconnect();
        return;
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      authStatus.set('ok');
      ayinStatus.set('connected');
      sseConnected.set(true);
      currentDelay = INITIAL_DELAY;

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const event = JSON.parse(line.slice(6));
              _handleEvent(event);
            } catch {
              // Not JSON — skip
            }
          }
        }
      }

      // Stream ended — reconnect
      sseConnected.set(false);
      ayinStatus.set('reconnecting');
      _scheduleReconnect();
    })
    .catch((err) => {
      if (err.name !== 'AbortError') {
        console.error('SSE connection error:', err);
        sseConnected.set(false);
        ayinStatus.set('offline');
        _scheduleReconnect();
      }
    });
}

/**
 * Map SSE event types to allowlist-safe source identifiers.
 *
 * Return values must satisfy `[A-Za-z0-9_-]` — validated server-side in
 * `context.rs::validate()` before prelude embedding. Any unknown EventType
 * falls through to `'Platform'`. When adding new EventType variants, add
 * a corresponding entry here to preserve semantic grouping in the context tray.
 */
function eventTypeToSource(type: EventType): string {
  const map: Partial<Record<EventType, string>> = {
    build_update:        'BuildRunner',
    finding:             'BuildRunner',
    plan_update:         'BuildRunner',
    pillar_update:       'CORSO',
    copilot_activity:    'Copilot',
    copilot_response:    'Copilot',
    context_status:      'Copilot',
    ayin_span:           'AYIN',
    ayin_status:         'AYIN',
    helix_entry:         'SOUL',
    soul_promotion:      'SOUL',
    sibling_status:      'Platform',
    strand_activation:   'Platform',
    gateway_notify:      'Platform',
    control:             'Platform',
    fs_mutation_pending: 'Platform',
    permission_request:  'Platform',
    strand_convergence:  'Platform',
    arena_update:        'Conductor',
    conductor_task:      'Conductor',
    conductor_tick:      'Conductor',
    worker_slot_gauge:   'Conductor',
    supervisor_update:   'Supervisor',
    scrum_report:        'Supervisor',
    escalation:          'Supervisor',
    training_progress:   'MLTraining',
    mailbox_message:     'Mailbox',
    gitforest_update:    'GitForest',
    merge_agent_status:  'MergeAgent',
    fix_agent_iteration: 'FixAgent',
  };
  return map[type] ?? 'Platform';
}

/** @internal Exposed for unit testing only */
export function _handleEvent(event: { type: EventType; data: unknown }): void {
  pushRecentEvent(eventTypeToSource(event.type), event.data);
  switch (event.type) {
    case 'ayin_status': {
      const status = event.data as string;
      if (status === 'connected' || status === 'reconnecting' || status === 'offline') {
        ayinStatus.set(status);
      }
      break;
    }
    case 'strand_activation': {
      const { sibling } = event.data as { sibling: SiblingId };
      spikeSibling(sibling);
      break;
    }
    case 'sibling_status': {
      const data = event.data as Record<string, unknown>;
      if (data.id && data.status) {
        siblingHealth.update(h => ({
          ...h,
          [data.id as SiblingId]: { ...h[data.id as SiblingId], ...data } as typeof h[SiblingId],
        }));
      }
      break;
    }
    case 'build_update': {
      const buildData = event.data as Partial<Build>;
      if (buildData.id) {
        builds.update(b => b.map(bld =>
          bld.id === buildData.id ? { ...bld, ...buildData } : bld
        ));
      }
      window.dispatchEvent(new CustomEvent('la:build-update'));
      break;
    }
    case 'pillar_update': {
      // Phase 15 — real CORSO shell-out payload. Fields are at the top
      // level (match the `helix_entry` convention, not the old `event.data`
      // wrapping used by control-plane events).
      const payload = event as unknown as PillarUpdatePayload & { type: 'pillar_update' };
      if (!payload.build_id) break;
      appendPillarUpdate(payload);
      // Mirror a coarse status into the builds store so the main list view
      // reflects in-flight pillar runs without re-subscribing to pillarStream.
      if (payload.phase === 'started' || payload.phase === 'completed') {
        const nextStatus = payload.phase === 'started'
          ? 'in_progress'
          : (payload.exit_code === 0 ? 'passed' : 'failed');
        builds.update(b => b.map(bld => {
          if (bld.id !== payload.build_id) return bld;
          const targetPillar = payload.pillar.toUpperCase();
          return {
            ...bld,
            pillars: bld.pillars.map(p =>
              p.pillar === targetPillar
                ? { ...p, status: nextStatus as typeof p['status'] }
                : p
            ),
          };
        }));
      }
      break;
    }
    case 'finding': {
      const findingData = event.data as Finding;
      if (findingData.id) {
        findings.update(f => {
          const exists = f.find(x => x.id === findingData.id);
          if (exists) return f.map(x => x.id === findingData.id ? findingData : x);
          return [...f, findingData];
        });
      }
      break;
    }
    case 'conductor_task': {
      const taskData = event.data as ConductorTask;
      if (taskData.id) {
        conductorTasks.update(t => {
          const exists = t.find(x => x.id === taskData.id);
          if (exists) return t.map(x => x.id === taskData.id ? taskData : x);
          return [...t, taskData];
        });
      }
      break;
    }
    case 'arena_update': {
      const arenaData = event.data as { agents?: ArenaAgent[]; activeRoutines?: number; queuedRoutines?: number };
      arenaStatus.update(a => ({
        ...a,
        ...(arenaData.activeRoutines !== undefined && { activeRoutines: arenaData.activeRoutines }),
        ...(arenaData.queuedRoutines !== undefined && { queuedRoutines: arenaData.queuedRoutines }),
        ...(arenaData.agents && { agents: arenaData.agents }),
        lastUpdate: new Date().toISOString(),
      }));
      break;
    }
    case 'gateway_notify': {
      // gateway_notify wraps raw payload from Claude's ui_* tool calls.
      // The outer type is always "gateway_notify"; inner type drives behavior.
      const notify = event as unknown as { type: 'gateway_notify'; payload: Record<string, unknown> };
      const { payload } = notify;
      if (payload.type === 'focus_pillar' && typeof payload.pillar === 'string') {
        selectedPillar.set(payload.pillar as Pillar);
      }
      // Other sub-types (refresh_sitrep, flag_finding, etc.) handled by
      // consuming components via their own SSE subscriptions if needed.
      break;
    }
    case 'helix_entry': {
      // Phase 9.3 — enriched helix_entry payload with front-matter.
      // Rust sends the full payload at the top level (not under .data), so
      // the event object itself IS the HelixEntrySsePayload minus `type`.
      const payload = event as unknown as HelixEntrySsePayload & { type: 'helix_entry' };
      helixEntries.update(list => {
        const next = [payload, ...list];
        return next.length > HELIX_ENTRIES_WINDOW ? next.slice(0, HELIX_ENTRIES_WINDOW) : next;
      });
      // Layer 2 — tag entries that arrive during an active build
      if (get(buildFocusActive)) {
        tagBuildAccess(payload.path);
      }
      break;
    }
    case 'soul_promotion': {
      // Phase 9.4 — hot→cold promotion notification.
      // Rust emits the PromotionEvent fields at the top level.
      const payload = event as unknown as SoulPromotionPayload & { type: 'soul_promotion' };
      promotionFeed.update(list => [payload, ...list].slice(0, 100));
      // Optimistic: remove from hot, insert at top of cold.
      hotMemory.update(list => list.filter(m => m.id !== payload.memo_id));
      coldMemory.update(list => {
        const newMemo: ContextMemo = {
          id: payload.path,
          tier: 'cold',
          content: '(just promoted — details loading…)',
          significance: payload.significance,
          sibling: payload.sibling,
          strands: [],
          created_at: payload.promoted_at,
          source_path: payload.path,
        };
        return [newMemo, ...list];
      });
      break;
    }
    case 'control': {
      // POST /api/control dispatches ControlCommand variants. Today we only
      // surface `notify` as a platform Alert; other variants (focus_panel,
      // resize_panels, set_helix_zoom, set_panel_visibility) are consumed by
      // their own subscribers at the component layer.
      const ctrl = event as unknown as { type: 'control'; command: string; message?: string; level?: string };
      if (ctrl.command === 'notify' && typeof ctrl.message === 'string') {
        const level = (ctrl.level ?? 'info').toLowerCase();
        // Map Rust's `level` strings to our AlertSeverity. Unknown → info.
        const sev: 'info' | 'warning' | 'error' | 'critical' =
          level === 'critical' ? 'critical' :
          level === 'error' ? 'error' :
          (level === 'warn' || level === 'warning') ? 'warning' : 'info';
        const alert = {
          id: `ctrl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          severity: sev,
          source: 'system' as const,
          title: sev === 'info' ? 'Notification' : sev.charAt(0).toUpperCase() + sev.slice(1),
          message: ctrl.message,
          timestamp: new Date().toISOString(),
          acknowledged: false,
        };
        alerts.update(list => [alert, ...list].slice(0, 200));
      }
      break;
    }
    case 'copilot_response': {
      // Streaming copilot chunks from the backend.
      // Backend uses #[serde(tag = "type")] — fields are inlined at the top level,
      // not nested under `data`. Read from `event` directly (same pattern as pillar_update).
      const resp = event as unknown as { chunk?: string; done?: boolean; sibling?: SiblingId };
      copilotMessages.update(msgs => {
        const last = msgs[msgs.length - 1];
        if (last && last.role === 'assistant') {
          // Append chunk to the existing assistant message
          return msgs.map((m, i) =>
            i === msgs.length - 1
              ? { ...m, content: m.content + (resp.chunk ?? '') }
              : m
          );
        }
        // No assistant message yet — create one
        return [...msgs, {
          id: crypto.randomUUID(),
          role: 'assistant' as const,
          content: resp.chunk ?? '',
          sibling: resp.sibling,
          timestamp: new Date().toISOString(),
        }];
      });
      if (resp.done) {
        copilotLoading.set(false);
      }
      break;
    }
    case 'copilot_activity': {
      // Phase 20 — live copilot subprocess stream-json events (thinking, tool_use, etc.)
      const payload = event as unknown as CopilotActivityEvent & { type: 'copilot_activity' };
      appendActivity({ source: 'copilot', event: payload });
      activityActive.set(payload.kind !== 'result');
      // Also spike the sibling wave for visual feedback
      spikeSibling('eva');
      break;
    }
    case 'ayin_span': {
      // Phase 20 — AYIN trace spans (MCP tool timing, decisions)
      const span = event as unknown as AyinSpanEvent & { type: 'ayin_span' };
      appendActivity({ source: 'ayin', span });
      // Spike the actor's sibling wave
      if (span.actor) {
        spikeSibling(span.actor as SiblingId);
      }
      // Phase 21 — detect supervisor decisions embedded in AYIN spans
      const supervisorAlert = extractSupervisorAlert(span);
      if (supervisorAlert) {
        appendSupervisorAlert(supervisorAlert);
      }
      // GitForest pulse: spans with metadata.branch_id pulse the associated forest node.
      const meta = span.metadata as Record<string, unknown> | null | undefined;
      const branchId = typeof meta?.branch_id === 'string' ? meta.branch_id : null;
      if (branchId) {
        gitforestPulses.update(ring => [branchId, ...ring].slice(0, 32));
      }
      break;
    }
    case 'context_status': {
      // squishy-dancing-thimble Phase D — context-window utilisation from CLI subprocess
      const payload = event as unknown as ContextStatusEvent & { type: 'context_status' };
      contextUsage.set(payload);
      break;
    }
    case 'plan_update': {
      // CORSO scout plan — full plan or phase-level update.
      // Full plan: { plan: ActivePlan }
      // Phase update: { plan_id: string, phase_id: number, status: PlanPhaseStatus }
      const payload = event.data as Record<string, unknown>;
      if (payload.plan && typeof payload.plan === 'object') {
        // Full plan replacement — validate minimum shape
        const p = payload.plan as Record<string, unknown>;
        if (typeof p.id === 'string' && typeof p.title === 'string' && Array.isArray(p.phases)) {
          activePlan.set(payload.plan as ActivePlan);
        }
      } else if (
        typeof payload.plan_id === 'string' &&
        typeof payload.phase_id === 'number' &&
        typeof payload.status === 'string'
      ) {
        // Incremental phase status update
        updatePlanPhase(
          payload.plan_id,
          payload.phase_id,
          payload.status as PlanPhaseStatus,
        );
      }
      break;
    }
    case 'scrum_report': {
      // /SCRUM output — structured squad review report
      const payload = event.data as Record<string, unknown>;
      const report: ScrumReport = {
        id: String(payload.id ?? `scrum-${Date.now()}`),
        title: String(payload.title ?? 'Squad Review'),
        timestamp: typeof payload.timestamp === 'number' ? payload.timestamp : Date.now(),
        findings: Array.isArray(payload.findings)
          ? (payload.findings as Record<string, unknown>[]).map(f => ({
              sibling: String(f.sibling ?? 'unknown'),
              category: (['good', 'gap', 'fix'].includes(String(f.category)) ? String(f.category) : 'gap') as ScrumFinding['category'],
              severity: f.severity && ['critical', 'high', 'medium', 'low', 'info'].includes(String(f.severity))
                ? String(f.severity) as ScrumFinding['severity'] : undefined,
              text: String(f.text ?? ''),
              file: f.file ? String(f.file) : undefined,
              line: typeof f.line === 'number' ? f.line : undefined,
            }))
          : [],
        consensus: payload.consensus ? String(payload.consensus) : undefined,
        conflicts: Array.isArray(payload.conflicts) ? (payload.conflicts as unknown[]).map(String) : undefined,
      };
      latestScrumReport.set(report);
      spikeSibling('corso');
      break;
    }
    case 'training_progress': {
      const payload = event.data as {
        id?: string;
        status?: TrainingRunStatus;
        progress?: number;
        results?: { score: number; exercises: number; passed: number };
      };
      trainingRun.update(current => {
        if (!current || (payload.id && current.id !== payload.id)) return current;
        const updated: TrainingRun = { ...current };
        if (payload.status !== undefined) updated.status = payload.status;
        if (payload.progress !== undefined) updated.progress = payload.progress;
        if (payload.results !== undefined) updated.results = payload.results;
        if (payload.status === 'complete') updated.completedAt = Date.now();
        return updated;
      });
      break;
    }
    case 'fs_mutation_pending': {
      // #47 — gate FS mutations behind operator approval. Forward as a custom
      // DOM event; DiffPreview.svelte listens and opens the modal. The shape
      // matches lib/diff-preview.ts::FsMutationPendingEvent.
      window.dispatchEvent(
        new CustomEvent('la:fs-mutation-pending', {
          detail: { type: 'fs_mutation_pending', ...(event.data as Record<string, unknown>) },
        }),
      );
      break;
    }
    case 'permission_request': {
      // E5 — generic tool-call permission gate. Emitted by the backend when
      // `AgentSessionHost::permission_queue` receives a new request.
      // Include the currentBuildId as dispatch_id so the approval endpoint
      // POST /api/dispatch/{build_id}/fs-approve can route to the right session.
      window.dispatchEvent(
        new CustomEvent('la:permission-request', {
          detail: {
            type: 'permission_request',
            dispatch_id: currentBuildId ?? '',
            ...(event.data as Record<string, unknown>),
          },
        }),
      );
      break;
    }
    case 'strand_convergence': {
      // Rust emits fields at top level (serde inline tag). Spike all siblings
      // participating in the convergence, then forward to the Memory drawer via
      // a DOM event so it can trigger a re-fetch of /soul/convergences.
      const conv = event as unknown as {
        type: 'strand_convergence';
        strand: string;
        siblings: string[];
        memo_ids: string[];
        detected_at: string;
      };
      if (Array.isArray(conv.siblings)) {
        conv.siblings.forEach(s => spikeSibling(s as SiblingId));
      }
      window.dispatchEvent(
        new CustomEvent('la:strand-convergence', { detail: conv }),
      );
      break;
    }
    case 'mailbox_message': {
      // P0-3: inter-agent message arriving via global SSE (cross-dispatch visibility).
      // The per-dispatch stream in SquadDispatch.svelte owns its own mailbox view;
      // this store provides a global unread badge for background dispatches.
      const msg = event as unknown as { type: 'mailbox_message'; dispatch_id?: string; agent: string; text: string };
      const newMsg = {
        id: `mbx-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        dispatchId: msg.dispatch_id,
        agent: msg.agent ?? 'unknown',
        text: msg.text ?? '',
        ts: Date.now(),
      };
      mailboxMessages.update(list => [newMsg, ...list].slice(0, 200));
      mailboxUnread.update(n => n + 1);
      break;
    }
    case 'gitforest_update': {
      // Payload: { repo: string; root: BranchNode } — branch tree rooted at main.
      // Phase 5 adds a full REST fetch on top; this gives an immediate partial update.
      const payload = event as unknown as { type: 'gitforest_update'; repo: string; root: import('./gitforest').BranchNode };
      void invalidateGitForestCache(payload.repo);
      gitforestTree.set(reconstructTopology(payload.repo, payload.root));
      // Pulse nodes with active worktrees.
      const activeIds = payload.root.worktrees
        .filter(w => w.state === 'writing' || w.state === 'gate')
        .map(() => payload.root.id);
      if (activeIds.length > 0) {
        gitforestPulses.update(ring => [...activeIds, ...ring].slice(0, 32));
      }
      break;
    }
    // ── ironclaw-spine lightsquad events (Phase 6) ─────────────────────────
    case 'escalation': {
      window.dispatchEvent(
        new CustomEvent('la:escalation', {
          detail: event as unknown as import('./types').EscalationEvent,
        }),
      );
      break;
    }
    case 'worker_slot_gauge': {
      const payload = event as unknown as import('./types').WorkerSlotGaugeEvent;
      workerSlots.set(payload);
      break;
    }
    case 'conductor_tick': {
      const payload = event as unknown as import('./types').ConductorTickEvent;
      conductorState.set(payload);
      break;
    }
    case 'merge_agent_status': {
      const payload = event as unknown as import('./types').MergeAgentStatusEvent;
      mergeAgentEvents.update(list => [payload, ...list].slice(0, 50));
      break;
    }
    case 'fix_agent_iteration': {
      const payload = event as unknown as import('./types').FixAgentIterationEvent;
      fixAgentEvents.update(list => [payload, ...list].slice(0, 100));
      break;
    }
    default:
      break;
  }
}

function _scheduleReconnect(): void {
  if (reconnectTimer) clearTimeout(reconnectTimer);
  reconnectTimer = setTimeout(() => {
    currentDelay = Math.min(currentDelay * 2, MAX_BACKOFF);
    _connect();
  }, currentDelay);
}

// Phase 10.9 — global SSE for helix_entry, soul_promotion, strand_activation.
// Independent of any per-build session so the MemoryDrawer + Helix3D
// visualization works without the Copilot drawer being open.
let globalAbort: AbortController | null = null;

export function connectGlobalSSE(): void {
  disconnectGlobalSSE();
  globalAbort = new AbortController();
  const { signal } = globalAbort;
  fetch('/api/events', { signal, headers: authHeaders() })
    .then(async (response) => {
      if (!response.ok || !response.body) {
        if (response.status === 401) { authStatus.set('unauthorized'); ayinStatus.set('offline'); return; }
        if (response.status === 403) { authStatus.set('forbidden');    ayinStatus.set('offline'); return; }
        if (!signal.aborted) _scheduleGlobalReconnect();
        return;
      }
      authStatus.set('ok');
      ayinStatus.set('connected');
      _resetGlobalBackoff();
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const event = JSON.parse(line.slice(6));
              _handleEvent(event);
            } catch {
              // skip
            }
          }
        }
      }
      // Stream ended (server restart, network blip) — reconnect with backoff.
      if (!signal.aborted) {
        _scheduleGlobalReconnect();
      }
    })
    .catch((err) => {
      // Reconnect unless deliberately aborted.
      if (err?.name !== 'AbortError' && !signal.aborted) {
        _scheduleGlobalReconnect();
      }
    });
}

let globalReconnectDelay = 1000;
let globalReconnectTimer: ReturnType<typeof setTimeout> | null = null;
const GLOBAL_MAX_BACKOFF = 30_000;

function _scheduleGlobalReconnect(): void {
  if (globalReconnectTimer) clearTimeout(globalReconnectTimer);
  globalReconnectTimer = setTimeout(() => {
    globalReconnectTimer = null;
    globalReconnectDelay = Math.min(globalReconnectDelay * 2, GLOBAL_MAX_BACKOFF);
    connectGlobalSSE();
  }, globalReconnectDelay);
}

function _resetGlobalBackoff(): void {
  globalReconnectDelay = 1000;
}

export function disconnectGlobalSSE(): void {
  if (globalReconnectTimer) {
    clearTimeout(globalReconnectTimer);
    globalReconnectTimer = null;
  }
  if (globalAbort) {
    globalAbort.abort();
    globalAbort = null;
  }
}

export function disconnectSSE(): void {
  sseConnected.set(false);
  if (abortController) {
    abortController.abort();
    abortController = null;
  }
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
}

/**
 * Reconnect the SSE stream for the given build.
 *
 * Delegates to {@link connectSSE} — exposed as a distinct export so
 * components can bind a retry button to a named action without importing
 * the lower-level {@link connectSSE} directly (SA-9: `reconnectSSE` does
 * NOT pre-exist; `connectSSE` is the canonical existing export).
 */
export function reconnectSSE(buildId: string): void {
  connectSSE(buildId);
}

// ── Topic-filtered SSE (webshell-event-bus-redesign Phase 2) ─────────────────

const TOPIC_INITIAL_DELAY = 1_000;
const TOPIC_MAX_BACKOFF   = 30_000;

/**
 * The shape of every envelope emitted by the gateway after Phase 1.
 *
 * The `type` discriminant (from the inner `WebEvent`) is present at the top
 * level via `#[serde(flatten)]`; `topic`, `timestamp`, `severity`, and
 * optional `build_id` are added by the `WebEventV2` wrapper.
 */
export interface WebEventV2 {
  /** NATS-style dot-path topic, e.g. `"v1.copilot.activity"`. */
  topic: string;
  timestamp: string;
  agent_id: string;
  build_id?: string;
  severity: 'info' | 'warn' | 'error';
  /** Inner `WebEvent` discriminant (e.g. `"copilot_activity"`). */
  type: string;
  [key: string]: unknown;
}

/**
 * Subscribe to the gateway SSE stream filtered to events matching a
 * NATS-style topic pattern.
 *
 * Pattern syntax:
 * - `*` — matches exactly one dot-separated segment
 * - `>` — matches one or more trailing segments (must be last)
 *
 * The filter is applied **server-side** via the `?topic=` query parameter.
 * Uses the same fetch-streaming + auth-header pattern as {@link connectSSE}
 * because native `EventSource` cannot send authorization headers.
 * Reconnects automatically with exponential back-off (1 s → 30 s cap).
 *
 * @example
 * ```ts
 * // Stream only copilot events for this component's lifetime.
 * const unsub = subscribeByTopic('v1.copilot.*', (ev) => console.log(ev));
 * onDestroy(unsub);
 *
 * // Stream everything under v1. (all events).
 * const unsub = subscribeByTopic('v1.>', handler);
 * ```
 *
 * @param pattern - NATS-style topic pattern (e.g. `"v1.copilot.*"`).
 * @param cb      - Called for every {@link WebEventV2} matching the pattern.
 * @returns A cleanup function — call it in `$effect` return or `onDestroy`.
 */
export function subscribeByTopic(
  pattern: string,
  cb: (event: WebEventV2) => void,
): () => void {
  const abortCtrl = new AbortController();
  let delay = TOPIC_INITIAL_DELAY;
  let reconnectHandle: ReturnType<typeof setTimeout> | null = null;

  // URLSearchParams handles percent-encoding of `>` → `%3E` automatically.
  const params = new URLSearchParams({ topic: pattern });

  async function connect(): Promise<void> {
    if (abortCtrl.signal.aborted) return;

    let response: Response;
    try {
      response = await fetch(`/api/events?${params.toString()}`, {
        signal: abortCtrl.signal,
        headers: authHeaders(),
      });
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError') return;
      schedule();
      return;
    }

    if (!response.ok || !response.body) {
      // Auth failures are terminal — don't retry until token changes.
      if (response.status === 401 || response.status === 403) return;
      schedule();
      return;
    }

    // Connection established — reset backoff.
    delay = TOPIC_INITIAL_DELAY;

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const parsed = JSON.parse(line.slice(6)) as WebEventV2;
              cb(parsed);
            } catch {
              // Non-JSON SSE comment or keep-alive — ignore.
            }
          }
        }
      }
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError') return;
    }

    // Stream ended — reconnect unless deliberately aborted.
    if (!abortCtrl.signal.aborted) {
      schedule();
    }
  }

  function schedule(): void {
    if (abortCtrl.signal.aborted) return;
    reconnectHandle = setTimeout(() => {
      delay = Math.min(delay * 2, TOPIC_MAX_BACKOFF);
      void connect();
    }, delay);
  }

  void connect();

  return () => {
    abortCtrl.abort();
    if (reconnectHandle !== null) {
      clearTimeout(reconnectHandle);
    }
  };
}

// ── Fleet SSE (agent-teams-fleet Phase 4A) ────────────────────────────────────

const FLEET_INITIAL_DELAY = 1_000;
const FLEET_MAX_BACKOFF   = 30_000;

/**
 * Subscribe to the per-build fleet SSE stream (`/api/builds/:id/fleet`).
 *
 * Uses the same fetch-streaming + auth-header pattern as {@link connectSSE}
 * because native `EventSource` does not support custom request headers.
 * Reconnects automatically with exponential back-off (1 s → 30 s cap).
 *
 * @param buildId  - UUID of the build whose fleet to observe.
 * @param cb       - Called for every parsed {@link FleetEvent}.
 * @returns A cleanup function — call it in `$effect` return or `onDestroy`.
 */
export function subscribeFleet(
  buildId: string,
  cb: (event: FleetEvent) => void,
): () => void {
  const abortCtrl = new AbortController();
  let delay = FLEET_INITIAL_DELAY;
  let reconnectHandle: ReturnType<typeof setTimeout> | null = null;

  async function connect(): Promise<void> {
    if (abortCtrl.signal.aborted) return;

    let response: Response;
    try {
      response = await fetch(`/api/builds/${buildId}/fleet`, {
        signal: abortCtrl.signal,
        headers: authHeaders(),
      });
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError') return;
      schedule();
      return;
    }

    if (!response.ok || !response.body) {
      // Auth failures are terminal — don't retry.
      if (response.status === 401 || response.status === 403) return;
      schedule();
      return;
    }

    // Connection established — reset backoff.
    delay = FLEET_INITIAL_DELAY;

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const parsed = JSON.parse(line.slice(6)) as FleetEvent;
              cb(parsed);
            } catch {
              // Non-JSON SSE comment or keep-alive — ignore.
            }
          }
        }
      }
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError') return;
    }

    // Stream ended — reconnect unless aborted.
    if (!abortCtrl.signal.aborted) {
      schedule();
    }
  }

  function schedule(): void {
    if (abortCtrl.signal.aborted) return;
    reconnectHandle = setTimeout(() => {
      delay = Math.min(delay * 2, FLEET_MAX_BACKOFF);
      void connect();
    }, delay);
  }

  void connect();

  return () => {
    abortCtrl.abort();
    if (reconnectHandle !== null) {
      clearTimeout(reconnectHandle);
    }
  };
}
