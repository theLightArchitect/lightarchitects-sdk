// ============================================================================
// SSE client — connect to per-build /api/builds/:id/events stream
// Uses fetch() instead of EventSource to support auth headers.
// ============================================================================

import type { EventType, Pillar } from './types';
import { authHeaders } from './auth';
import {
  ayinStatus, siblingHealth, waves, builds, findings,
  conductorTasks, arenaStatus, alerts, selectedPillar,
  copilotMessages, copilotLoading, buildFocusActive,
  helixEntries, promotionFeed, hotMemory, coldMemory,
  appendPillarUpdate, appendActivity, activityActive,
  appendSupervisorAlert,
  tagBuildAccess,
  activePlan, updatePlanPhase,
  latestScrumReport,
  trainingRun,
} from './stores';
import { spikeSibling } from './stores';
import { get } from 'svelte/store';
import type {
  SiblingId, Build, Finding, ConductorTask, ArenaAgent,
  HelixEntrySsePayload, SoulPromotionPayload, ContextMemo,
  PillarUpdatePayload, CopilotActivityEvent, AyinSpanEvent,
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
        _scheduleReconnect();
        return;
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      ayinStatus.set('connected');
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
      ayinStatus.set('reconnecting');
      _scheduleReconnect();
    })
    .catch((err) => {
      if (err.name !== 'AbortError') {
        console.error('SSE connection error:', err);
        ayinStatus.set('offline');
        _scheduleReconnect();
      }
    });
}

/** @internal Exposed for unit testing only */
export function _handleEvent(event: { type: EventType; data: unknown }): void {
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
      // Payload: { chunk?: string, done?: boolean, sibling?: SiblingId }
      const resp = event.data as { chunk?: string; done?: boolean; sibling?: SiblingId };
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
      break;
    }
    case 'supervisor_decision': {
      // Phase 21 — dedicated supervisor decision event (direct from CORSO)
      const payload = event as unknown as {
        type: 'supervisor_decision';
        id?: string;
        sibling?: string;
        gate?: string;
        verdict?: string;
        message?: string;
        details?: string;
        timestamp?: string;
      };
      const gate = (payload.gate ?? 'guard').toLowerCase();
      const validGates = new Set(['guard', 'alpha', 'quality', 'canon']);
      const verdict = (payload.verdict ?? 'PASS').toUpperCase();
      const validVerdicts = new Set(['PASS', 'FAIL', 'WARN']);
      const alert: SupervisorAlert = {
        id: payload.id ?? `sv-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        timestamp: payload.timestamp ? new Date(payload.timestamp).getTime() : Date.now(),
        sibling: payload.sibling ?? 'corso',
        gate: (validGates.has(gate) ? gate : 'guard') as SupervisorGate,
        verdict: (validVerdicts.has(verdict) ? verdict : 'PASS') as SupervisorVerdict,
        message: (payload.message ?? `CORSO ${gate.toUpperCase()}: ${verdict}`).slice(0, 500),
        details: payload.details ? String(payload.details).slice(0, 4096) : undefined,
      };
      appendSupervisorAlert(alert);
      spikeSibling('corso');
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
      if (!response.ok || !response.body) return;
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
  if (abortController) {
    abortController.abort();
    abortController = null;
  }
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
}
