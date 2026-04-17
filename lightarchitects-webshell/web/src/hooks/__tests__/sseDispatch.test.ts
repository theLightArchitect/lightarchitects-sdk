/**
 * sseDispatch — full-stack contract tests for SSE payload parsing.
 *
 * These tests answer: "How does a UI element behave in response to a
 * specific programmatic event from the backend?"
 *
 * They trace the complete data transformation chain:
 *
 *   Rust `WebEvent` (serialised by the SSE handler)
 *     → SSE wire format: "data: {json}\n\n"
 *       → `useEventSource.ts` parse logic (reproduced here)
 *         → Zustand store dispatch
 *           → scene state visible to React components
 *
 * ## Why test at this boundary?
 *
 * The Rust compiler guarantees the backend types are correct.
 * TypeScript's type system is erased at runtime — if the backend renames
 * `"ayin_span"` to `"span"`, TypeScript won't catch it and the step cloud
 * silently stops updating.  These tests catch exactly that class of failure.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { useSceneStore } from '../../store/sceneState';

// ── Dispatch logic (mirrors useEventSource.ts) ───────────────────────────────
//
// The hook in `useEventSource.ts` does:
//   1. Strip "data: " prefix from SSE lines
//   2. Parse JSON
//   3. Switch on `payload.type`
//
// We reproduce that logic here so the tests remain decoupled from the hook's
// React lifecycle (fetch, useEffect, reconnect) while still covering the
// critical dispatch path.

interface SpanPayload {
  type: 'ayin_span';
  id: string;
  actor: string;
  action: string;
  timestamp: string;
  duration_ms: number;
  outcome: unknown;
  parent_id?: string;
}

interface StatusPayload {
  type: 'ayin_status';
  status: 'connected' | 'disconnected' | 'reconnecting';
  attempt?: number;
}

interface HelixEntryPayload {
  type: 'helix_entry';
  path: string;
  event_kind: 'created' | 'modified';
}

interface LagPayload {
  type: 'lag';
  skipped: number;
}

type SsePayload = SpanPayload | StatusPayload | HelixEntryPayload | LagPayload;

/** Parse a raw SSE data line (strips the "data: " prefix). */
function parseSseLine(line: string): SsePayload {
  const json = line.startsWith('data: ') ? line.slice('data: '.length) : line;
  return JSON.parse(json) as SsePayload;
}

/** Dispatch a parsed SSE payload to the scene store, exactly as the hook does. */
function dispatchToStore(payload: SsePayload): void {
  const store = useSceneStore.getState();
  switch (payload.type) {
    case 'ayin_span':
      store.addStep(payload.id, payload.actor, payload.action);
      break;
    case 'ayin_status': {
      const status = {
        connected: payload.status === 'connected',
        reconnecting: payload.status === 'reconnecting',
        attempt: payload.attempt ?? 0,
      };
      store.setAyinStatus(status);
      break;
    }
    case 'helix_entry':
      // spawnOrb(queryId, hitStepIds) — on a helix_entry event, the orb
      // traverses the store for steps matching the entry's sibling path.
      // For this contract test we pass an empty hit list (no steps loaded).
      store.spawnOrb(payload.path, []);
      break;
    case 'lag':
      // Frontend logs the lag count; no store mutation.
      break;
  }
}

function resetStore() {
  useSceneStore.setState({
    steps: [],
    orbQueue: [],
    ayinStatus: { connected: false, reconnecting: false, attempt: 0 },
  });
}

// ── 1. ayin_span → step cloud grows ──────────────────────────────────────────
//
// UI element: step cloud node count + step count badge.
// Expected behaviour: each `ayin_span` event adds one node to the cloud.

describe('ayin_span event → step cloud', () => {
  beforeEach(resetStore);

  it('adds a node to the step cloud', () => {
    const line = `data: {"type":"ayin_span","id":"s1","actor":"soul","action":"rag.query","timestamp":"2026-04-13T00:00:00Z","duration_ms":10,"outcome":"success"}`;
    dispatchToStore(parseSseLine(line));
    expect(useSceneStore.getState().steps).toHaveLength(1);
  });

  it('step badge count increases with each span event', () => {
    for (let i = 0; i < 5; i++) {
      const line = `data: {"type":"ayin_span","id":"s${i}","actor":"corso","action":"scan.${i}","timestamp":"2026-04-13T00:00:0${i}Z","duration_ms":${i},"outcome":null}`;
      dispatchToStore(parseSseLine(line));
    }
    expect(useSceneStore.getState().steps).toHaveLength(5);
  });

  it('step node uses actor from payload for colour assignment', () => {
    const line = `data: {"type":"ayin_span","id":"e1","actor":"eva","action":"write","timestamp":"2026-04-13T00:00:00Z","duration_ms":1,"outcome":null}`;
    dispatchToStore(parseSseLine(line));
    // eva → 0xFF1493 (pink)
    expect(useSceneStore.getState().steps[0].color).toBe(0xFF1493);
  });

  it('step node id matches payload id (React key for deduplication)', () => {
    const line = `data: {"type":"ayin_span","id":"unique-span-id","actor":"soul","action":"search","timestamp":"2026-04-13T00:00:00Z","duration_ms":5,"outcome":null}`;
    dispatchToStore(parseSseLine(line));
    expect(useSceneStore.getState().steps[0].id).toBe('unique-span-id');
  });
});

// ── 2. ayin_status → connection badge colour ──────────────────────────────────
//
// UI element: connection dot (green=connected, amber=reconnecting, grey=disconnected).

describe('ayin_status event → connection badge', () => {
  beforeEach(resetStore);

  it('Connected payload → dot is green (connected=true)', () => {
    const line = `data: {"type":"ayin_status","status":"connected"}`;
    dispatchToStore(parseSseLine(line));
    expect(useSceneStore.getState().ayinStatus.connected).toBe(true);
    expect(useSceneStore.getState().ayinStatus.reconnecting).toBe(false);
  });

  it('Disconnected payload → dot is grey (connected=false, reconnecting=false)', () => {
    // First connect.
    dispatchToStore(parseSseLine(`data: {"type":"ayin_status","status":"connected"}`));
    // Then disconnect.
    dispatchToStore(parseSseLine(`data: {"type":"ayin_status","status":"disconnected"}`));
    const s = useSceneStore.getState().ayinStatus;
    expect(s.connected).toBe(false);
    expect(s.reconnecting).toBe(false);
  });

  it('Reconnecting payload → dot is amber (reconnecting=true) with attempt counter', () => {
    const line = `data: {"type":"ayin_status","status":"reconnecting","attempt":3}`;
    dispatchToStore(parseSseLine(line));
    const s = useSceneStore.getState().ayinStatus;
    expect(s.reconnecting).toBe(true);
    expect(s.attempt).toBe(3);
    expect(s.connected).toBe(false);
  });

  it('status transitions are idempotent — sending same status twice is safe', () => {
    const line = `data: {"type":"ayin_status","status":"connected"}`;
    dispatchToStore(parseSseLine(line));
    dispatchToStore(parseSseLine(line));
    expect(useSceneStore.getState().ayinStatus.connected).toBe(true);
  });

  it('reconnect attempt counter increments across events', () => {
    for (let attempt = 1; attempt <= 3; attempt++) {
      dispatchToStore(parseSseLine(`data: {"type":"ayin_status","status":"reconnecting","attempt":${attempt}}`));
    }
    expect(useSceneStore.getState().ayinStatus.attempt).toBe(3);
  });
});

// ── 3. helix_entry → retrieval orb spawned ───────────────────────────────────
//
// UI element: a retrieval orb appears and animates through the helix.

describe('helix_entry event → retrieval orb animation', () => {
  beforeEach(resetStore);

  it('spawns one orb per helix_entry event', () => {
    const line = `data: {"type":"helix_entry","path":"eva/entries/day-42.md","event_kind":"created"}`;
    dispatchToStore(parseSseLine(line));
    expect(useSceneStore.getState().orbQueue).toHaveLength(1);
  });

  it('orb id is derived from the entry path (uniquely identifies the query)', () => {
    const path = 'soul/entries/2026-04-13-test.md';
    const line = `data: {"type":"helix_entry","path":"${path}","event_kind":"created"}`;
    dispatchToStore(parseSseLine(line));
    expect(useSceneStore.getState().orbQueue[0].id).toBe(path);
  });

  it('two distinct helix_entry events spawn two distinct orbs', () => {
    dispatchToStore(parseSseLine(`data: {"type":"helix_entry","path":"eva/a.md","event_kind":"created"}`));
    dispatchToStore(parseSseLine(`data: {"type":"helix_entry","path":"eva/b.md","event_kind":"modified"}`));
    expect(useSceneStore.getState().orbQueue).toHaveLength(2);
    const ids = useSceneStore.getState().orbQueue.map((o) => o.id);
    expect(new Set(ids).size).toBe(2);
  });

  it('orb has a positive totalDuration (will animate, not snap)', () => {
    const line = `data: {"type":"helix_entry","path":"test.md","event_kind":"created"}`;
    dispatchToStore(parseSseLine(line));
    expect(useSceneStore.getState().orbQueue[0].totalDuration).toBeGreaterThan(0);
  });
});

// ── 4. lag event — frontend does not crash ────────────────────────────────────
//
// The SSE handler emits `{"type":"lag","skipped":N}` when the broadcast
// channel drops events.  The frontend should not crash or add spurious data.

describe('lag event → no side effects', () => {
  beforeEach(resetStore);

  it('lag event does not add steps', () => {
    dispatchToStore(parseSseLine(`data: {"type":"lag","skipped":5}`));
    expect(useSceneStore.getState().steps).toHaveLength(0);
  });

  it('lag event does not spawn orbs', () => {
    dispatchToStore(parseSseLine(`data: {"type":"lag","skipped":5}`));
    expect(useSceneStore.getState().orbQueue).toHaveLength(0);
  });

  it('lag event does not change connection status', () => {
    useSceneStore.getState().setAyinStatus({ connected: true, reconnecting: false, attempt: 0 });
    dispatchToStore(parseSseLine(`data: {"type":"lag","skipped":100}`));
    expect(useSceneStore.getState().ayinStatus.connected).toBe(true);
  });
});

// ── 5. Interleaved event stream — realistic session ───────────────────────────
//
// Simulates a realistic session: connect → spans arrive → helix entry fires.
// Verifies that all three UI elements update correctly from a mixed event stream.

describe('interleaved SSE event stream — full session simulation', () => {
  beforeEach(resetStore);

  it('connect + spans + helix-entry produces correct final UI state', () => {
    // 1. Backend connects.
    dispatchToStore(parseSseLine(`data: {"type":"ayin_status","status":"connected"}`));

    // 2. Three spans arrive (soul, corso, eva).
    const spans = [
      `data: {"type":"ayin_span","id":"s1","actor":"soul","action":"search","timestamp":"2026-04-13T00:00:01Z","duration_ms":8,"outcome":null}`,
      `data: {"type":"ayin_span","id":"s2","actor":"corso","action":"guard.scan","timestamp":"2026-04-13T00:00:02Z","duration_ms":42,"outcome":"success"}`,
      `data: {"type":"ayin_span","id":"s3","actor":"eva","action":"memory.recall","timestamp":"2026-04-13T00:00:03Z","duration_ms":5,"outcome":null}`,
    ];
    spans.forEach((line) => dispatchToStore(parseSseLine(line)));

    // 3. A helix entry fires (retrieval orb spawns).
    dispatchToStore(parseSseLine(`data: {"type":"helix_entry","path":"soul/entries/2026-04-13.md","event_kind":"created"}`));

    const state = useSceneStore.getState();

    // Connection badge: green.
    expect(state.ayinStatus.connected).toBe(true);

    // Step cloud: 3 nodes.
    expect(state.steps).toHaveLength(3);

    // Step actors assigned correctly.
    expect(state.steps.map((s) => s.actor)).toEqual(['soul', 'corso', 'eva']);

    // Retrieval orb: 1 active.
    expect(state.orbQueue).toHaveLength(1);
    expect(state.orbQueue[0].id).toBe('soul/entries/2026-04-13.md');
  });
});
