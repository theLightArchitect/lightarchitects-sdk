// Per-session Lightspace SSE client.
//
// Uses fetch() (not EventSource) so custom Authorization headers can be sent.
// On `lag` sentinel from the server: fetches a full snapshot and resets stores.
// subscribe-before-dispatch invariant: maintained by the server (Wave 2b).

import {
  canvasAttachCard, canvasDetachCard, canvasUpdateCard, canvasReset,
  drawerAttachFile, drawerDetachFile, hitlEnqueue, hitlDequeue, materializePhase,
} from './stores';
import type {
  LightspaceCardEvent, LightspaceLifecycleEvent, LightspaceUpdateEvent,
  LightspaceGraduateEvent, LightspaceMaterializeEvent, LightspaceGatingEvent,
  LightspaceBranchLaneEvent, LightspaceConfidenceEvent,
  LightspaceDrawerFileEvent, LightspaceDrawerEventPayload, CanvasSnapshot,
  LightspaceContradictionResolutionEvent,
} from './types';

type HeadersFn = () => Record<string, string>;

/** Subscribe to the per-session Lightspace SSE stream.
 *
 * @param sessionId  UUID of the Lightspace session (UUIDv7 recommended).
 * @param getHeaders Function returning auth headers (e.g. `authHeaders` from $lib/auth).
 * @returns          Cleanup function — call on component destroy to stop the stream.
 */
export function subscribeSession(sessionId: string, getHeaders: HeadersFn): () => void {
  let stopped = false;
  const controller = new AbortController();
  const INITIAL_DELAY = 1_000;
  const MAX_BACKOFF   = 30_000;
  let currentDelay    = INITIAL_DELAY;

  void (async () => {
    while (!stopped) {
      try {
        const res = await fetch(`/api/lightspace/${sessionId}/events`, {
          headers: getHeaders(),
          signal: controller.signal,
        });
        if (!res.ok || !res.body) {
          await sleep(currentDelay);
          currentDelay = Math.min(currentDelay * 2, MAX_BACKOFF);
          continue;
        }
        await readStream(res.body, sessionId, getHeaders);
        // Clean exit from readStream — reset backoff for next reconnect.
        currentDelay = INITIAL_DELAY;
      } catch (e) {
        if (!stopped && !(e instanceof DOMException && e.name === 'AbortError')) {
          await sleep(currentDelay);
          currentDelay = Math.min(currentDelay * 2, MAX_BACKOFF);
        }
      }
    }
  })();

  return () => {
    stopped = true;
    controller.abort();
  };
}

async function readStream(
  body: ReadableStream<Uint8Array>,
  sessionId: string,
  getHeaders: HeadersFn,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buf = '';

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });

      // SSE events are separated by \n\n
      const parts = buf.split('\n\n');
      buf = parts.pop() ?? '';
      for (const block of parts) {
        const dataLine = block.split('\n').find(l => l.startsWith('data: '));
        if (!dataLine) continue;
        const json = dataLine.slice(6).trim();
        if (!json) continue;
        await dispatch(json, sessionId, getHeaders);
      }
    }
  } finally {
    reader.releaseLock();
  }
}

async function dispatch(
  raw: string,
  sessionId: string,
  getHeaders: HeadersFn,
): Promise<void> {
  let e: Record<string, unknown>;
  try {
    e = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    return;
  }

  // Lag sentinel: server dropped events; fetch snapshot to resync
  if (e['type'] === 'lag') {
    await fetchSnapshot(sessionId, getHeaders);
    return;
  }

  const topic = String(e['topic'] ?? '');

  switch (topic) {
    case 'v1.lightspace.canvas.card': {
      const ev = e as unknown as LightspaceCardEvent;
      if (ev.card) canvasAttachCard(ev.card);
      break;
    }
    case 'v1.lightspace.canvas.lifecycle': {
      const ev = e as unknown as LightspaceLifecycleEvent;
      if (ev.transition === 'detach') canvasDetachCard(ev.card_id);
      else canvasAttachCard({ id: ev.card_id, kind: 'trace', title: '', state: 'attached', content: {}, provenance: { agent: ev.actor, source: 'lifecycle' } });
      break;
    }
    case 'v1.lightspace.canvas.update': {
      const ev = e as unknown as LightspaceUpdateEvent;
      canvasUpdateCard(ev.card_id, ev.seq, ev.mode, ev.path, ev.payload);
      break;
    }
    case 'v1.lightspace.canvas.graduate': {
      const ev = e as unknown as LightspaceGraduateEvent;
      if (ev.retain_tombstone) {
        canvasUpdateCard(ev.card_id, Date.now(), 'replace', '/state', 'detached');
      } else {
        canvasDetachCard(ev.card_id);
      }
      drawerAttachFile({ id: ev.file_id, mime_type: ev.content_mime, content_uri: ev.content_uri, size_bytes: 0, provenance: { agent: 'graduate', source: ev.card_id } });
      break;
    }
    case 'v1.lightspace.workspace.materialize': {
      const ev = e as unknown as LightspaceMaterializeEvent;
      const phase = ev.phase >= 255 ? 'complete' : ev.phase === 0 ? 'begin' : 'canvas';
      materializePhase.set(phase);
      break;
    }
    case 'v1.lightspace.canvas.gating': {
      const _ev = e as unknown as LightspaceGatingEvent;
      // Gating state is stored in the card's content via update events; no separate store needed.
      break;
    }
    case 'v1.lightspace.canvas.branch_lane': {
      const ev = e as unknown as LightspaceBranchLaneEvent;
      canvasUpdateCard(ev.card_id, Date.now(), 'replace', '/lanes', ev.lanes);
      break;
    }
    case 'v1.lightspace.canvas.confidence': {
      const _ev = e as unknown as LightspaceConfidenceEvent;
      // Confidence info is rendered via ContradictionBadge on the target card.
      break;
    }
    case 'v1.lightspace.canvas.contradiction_resolution': {
      const ev = e as unknown as LightspaceContradictionResolutionEvent;
      canvasUpdateCard(ev.winner_target_id, Date.now(), 'replace', '/badge', null);
      ev.loser_target_ids.forEach(id =>
        canvasUpdateCard(id, Date.now(), 'replace', '/badge', { kind: 'resolved', seq: ev.seq }),
      );
      break;
    }
    case 'ironclaw_hitl_escalation': {
      const ev = e as unknown as { nonce: string; layer_failed?: number; escalation_question: string };
      hitlEnqueue({ id: ev.nonce, gate: String(ev.layer_failed ?? ''), label: ev.escalation_question, inserted_at: Date.now() });
      break;
    }
    case 'ironclaw_hitl_resolution': {
      hitlDequeue((e as unknown as { nonce: string }).nonce);
      break;
    }
    case 'v1.lightspace.drawer.file': {
      const ev = e as unknown as LightspaceDrawerFileEvent;
      if (ev.file) drawerAttachFile(ev.file);
      break;
    }
    case 'v1.lightspace.drawer.event': {
      const ev = e as unknown as LightspaceDrawerEventPayload;
      if (ev.action === 'detach') drawerDetachFile(ev.file_id);
      break;
    }
    default:
      break;
  }
}

async function fetchSnapshot(sessionId: string, getHeaders: HeadersFn): Promise<void> {
  try {
    const res = await fetch(`/api/lightspace/${sessionId}/snapshot`, { headers: getHeaders() });
    if (!res.ok) return;
    const snapshot = await res.json() as CanvasSnapshot;
    canvasReset(snapshot);
  } catch {
    // Snapshot fetch failure is non-fatal; SSE will resume
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise(r => setTimeout(r, ms));
}
