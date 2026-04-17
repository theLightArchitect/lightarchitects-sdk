// ============================================================================
// File: web-figma/src/engineering/hooks/useEventSource.ts
// Territory: ENGINEERING — not Figma Make synced
// Purpose: SSE stream adapter — fetch() based (EventSource lacks auth headers)
// Security: reads token from hash fragment / sessionStorage only
// ============================================================================

import { useEffect, useRef } from 'react';
import type { StrandActivationEvent, AyinSpanEvent, AyinConnStatus } from '../store/sceneState';

export interface EventCallbacks {
  onStrandActivation: (event: StrandActivationEvent) => void;
  onAyinStatus: (status: AyinConnStatus) => void;
  /** Called for every ayin_span event (e.g. to add a helix step). */
  onAyinSpan?: (span: AyinSpanEvent) => void;
  /** Called for every helix_entry event (e.g. to spawn a retrieval orb). */
  onHelixEntry?: () => void;
}

/** Reads token from URL hash then sessionStorage; strips hash from URL bar. */
function resolveToken(): string {
  const params = new URLSearchParams(window.location.hash.slice(1));
  const fromHash = params.get('token');
  if (fromHash) {
    sessionStorage.setItem('webshell_token', fromHash);
    window.history.replaceState(null, '', window.location.pathname + window.location.search);
    return fromHash;
  }
  return sessionStorage.getItem('webshell_token') ?? '';
}

/** Parses one SSE `data:` line and invokes the matching callback. */
function handleLine(line: string, cb: EventCallbacks): void {
  if (!line.startsWith('data: ')) return;
  let msg: Record<string, unknown>;
  try {
    msg = JSON.parse(line.slice(6)) as Record<string, unknown>;
  } catch {
    return;
  }

  const type = msg['type'];
  if (type === 'strand_activation') {
    cb.onStrandActivation({
      sibling: String(msg['sibling'] ?? ''),
      strand: String(msg['strand'] ?? ''),
      weight: Number(msg['weight'] ?? 0),
      timestamp: String(msg['timestamp'] ?? ''),
    });
  } else if (type === 'ayin_status') {
    const status = msg['status'];
    cb.onAyinStatus(
      status === 'connected' ? 'connected'
      : status === 'reconnecting' ? 'reconnecting'
      : 'offline',
    );
  } else if (type === 'ayin_span' && cb.onAyinSpan) {
    cb.onAyinSpan({
      id: String(msg['id'] ?? crypto.randomUUID()),
      actor: String(msg['actor'] ?? 'unknown'),
      action: String(msg['action'] ?? ''),
      timestamp: String(msg['timestamp'] ?? ''),
      durationMs: Number(msg['duration_ms'] ?? 0),
    });
  } else if (type === 'helix_entry' && cb.onHelixEntry) {
    cb.onHelixEntry();
  }
}

async function driveStream(url: string, token: string, cb: EventCallbacks, signal: AbortSignal): Promise<void> {
  const res = await fetch(url, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
    signal,
  });
  if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);

  cb.onAyinStatus('connected');
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = '';

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    let boundary: number;
    while ((boundary = buf.indexOf('\n\n')) !== -1) {
      const block = buf.slice(0, boundary);
      buf = buf.slice(boundary + 2);
      for (const line of block.split('\n')) handleLine(line, cb);
    }
  }
}

/** Connects to `/api/events` with exponential-backoff retry. */
export function useEventSource(callbacks: EventCallbacks): void {
  // Stable ref — effect runs once; always calls the latest callbacks.
  const cbRef = useRef(callbacks);
  cbRef.current = callbacks;

  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();

    async function connect(attempt: number): Promise<void> {
      if (cancelled) return;
      cbRef.current.onAyinStatus(attempt > 0 ? 'reconnecting' : 'offline');
      const delay = Math.min(1000 * (1 << Math.min(attempt, 5)), 30_000);
      try {
        await driveStream('/api/events', resolveToken(), cbRef.current, controller.signal);
      } catch (err) {
        if (cancelled) return;
        console.warn(`[sse] attempt ${attempt} failed:`, err);
        await new Promise((r) => setTimeout(r, delay));
      }
      if (!cancelled) void connect(attempt + 1);
    }

    void connect(0);
    return () => { cancelled = true; controller.abort(); };
  }, []);
}
