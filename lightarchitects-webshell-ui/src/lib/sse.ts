// ============================================================================
// SSE client — connect to per-build /api/builds/:id/events stream
// Uses fetch() instead of EventSource to support auth headers.
// ============================================================================

import type { EventType, Pillar } from './types';
import { authHeaders } from './auth';
import {
  ayinStatus, siblingHealth, waves, builds, findings,
  conductorTasks, arenaStatus, alerts, selectedPillar,
  copilotMessages, copilotLoading,
  helixEntries, promotionFeed, hotMemory, coldMemory,
  appendPillarUpdate,
} from './stores';
import { spikeSibling } from './stores';
import type {
  SiblingId, Build, Finding, ConductorTask, ArenaAgent,
  HelixEntrySsePayload, SoulPromotionPayload, ContextMemo,
  PillarUpdatePayload,
} from './types';

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
    })
    .catch(() => {
      // Silent — reconnect on next call.
    });
}

export function disconnectGlobalSSE(): void {
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
