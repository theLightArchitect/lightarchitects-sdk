// Lightspace conversation API client.
//
// Wraps the 5 backend routes under /api/conversation.
// SSE subscription uses EventSource (cookie auth via la_session).

import { authHeaders } from '$lib/auth';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface ConvSSEEvent {
  type: 'activity' | 'strategy_phase' | 'hitl_pause' | 'done' | 'error' | 'lag';
  /** Present on `activity` — forwarded CopilotActivityEvent shape. */
  kind?: string;
  summary?: string;
  /** Present on `strategy_phase`. */
  phase?: string;
  strategy?: string;
  /** Present on `hitl_pause`. */
  nonce?: string;
  prompt?: string;
  /** Present on `done`. */
  turn_id?: string;
  /** Present on `error`. */
  message?: string;
  /** Present on `lag`. */
  skipped?: number;
}

// ── API helpers ───────────────────────────────────────────────────────────────

/**
 * POST /api/conversation — create a session.
 * Returns the new session UUID.
 * Throws on network or HTTP error.
 */
export async function createConversation(): Promise<string> {
  const res = await fetch('/api/conversation', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: '{}',
  });
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText);
    throw new Error(`Failed to create session (${res.status}): ${text}`);
  }
  const data = (await res.json()) as { session_id: string };
  return data.session_id;
}

/**
 * POST /api/conversation/{id} — dispatch a new turn.
 * Returns immediately (202 Accepted); events arrive on the SSE stream.
 * Throws on network or HTTP error.
 */
export async function sendTurn(sessionId: string, message: string): Promise<void> {
  const res = await fetch(`/api/conversation/${sessionId}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ message }),
  });
  if (!res.ok) {
    if (res.status === 409) throw new Error('A turn is already running — wait for it to complete.');
    const text = await res.text().catch(() => res.statusText);
    throw new Error(`Turn dispatch failed (${res.status}): ${text}`);
  }
}

/**
 * DELETE /api/conversation/{id} — end a session.
 * Best-effort; non-blocking (fire and forget).
 */
export function endConversation(sessionId: string): void {
  fetch(`/api/conversation/${sessionId}`, {
    method: 'DELETE',
    headers: authHeaders(),
  }).catch(() => {/* best-effort cleanup */});
}

// ── SSE subscription ──────────────────────────────────────────────────────────

/**
 * Subscribe to the conversation SSE stream.
 *
 * Opens `EventSource` on `/api/conversation/{id}/stream`.
 * Calls `onEvent` for each parsed event, `onError` for stream errors.
 * Returns a cleanup function that closes the EventSource.
 *
 * Auth note: EventSource cannot set custom headers — authentication relies
 * on the `la_session` HttpOnly cookie set by `/api/auth/exchange`.
 */
export function subscribeConversation(
  sessionId: string,
  onEvent: (event: ConvSSEEvent) => void,
  onError: (message: string) => void,
): () => void {
  const es = new EventSource(`/api/conversation/${sessionId}/stream`);

  es.onmessage = (ev) => {
    try {
      const parsed = JSON.parse(ev.data as string) as ConvSSEEvent;
      onEvent(parsed);
    } catch {
      // Malformed SSE frame — skip silently.
    }
  };

  es.onerror = () => {
    if (es.readyState === EventSource.CLOSED) {
      onError('Connection to server lost — session may have ended.');
    }
    // CONNECTING state is a temporary network hiccup; EventSource retries automatically.
  };

  return () => es.close();
}
