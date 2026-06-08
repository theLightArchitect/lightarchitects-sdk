# API Contract — lightspace-conversation-api

All 5 endpoints sit under the auth middleware (same as existing webshell routes).
Auth failure → 401 before reaching any handler.

---

## Endpoints

### POST /api/conversation

Create a new conversation session.

**Request body** (JSON, ≤32KB):
```json
{ "intent": "react audit auth" }
```
- `intent` (optional string) — the operator's initial intent text (logged to session; not dispatched as a turn)

**Response 200**:
```json
{ "session_id": "550e8400-e29b-41d4-a716-446655440000", "stream_url": "/api/conversation/550e8400-e29b-41d4-a716-446655440000/stream" }
```

---

### GET /api/conversation/{id}/stream

Subscribe to the SSE event stream for an existing session.

**Path**: `{id}` must be a valid UUID (CWE-20: malformed UUID → 400)

**Response**: `text/event-stream` — SSE frames, one per `ConvSSEEvent`

**SSE frame format**:
```
data: {"type":"activity","timestamp":"...","text":"...","kind":"reasoning"}\n\n
data: {"type":"strategy_phase","phase":"analyze","strategy":"react"}\n\n
data: {"type":"hitl_pause","nonce":"abc123","prompt":"Continue?"}\n\n
data: {"type":"done","turn_id":"abc-def-..."}\n\n
data: {"type":"error","message":"LLM not configured — check LiteLLM settings"}\n\n
```

**Keepalive**: `: ping\n\n` every 15s when idle

---

### POST /api/conversation/{id}

Send a message (trigger a new turn).

**Request body** (JSON, ≤32KB): `{ "message": "/react audit auth flow" }`

**Strategy routing**: message starting with `/<name>` where `name` ∈ `StrategyRegistry::all_names()` → strategy dispatch; otherwise → native turn.

**Response 202 Accepted** | **404** session not found | **409** turn already in progress

---

### POST /api/conversation/{id}/interrupt

Interrupt the current active turn. Response 200 (idempotent) | 404.

---

### DELETE /api/conversation/{id}

End the session. Response 204 | 404 (idempotent).

---

## ConvSSEEvent discriminated union (LOCKED after Phase 1)

```rust
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConvSSEEvent {
    Activity(CopilotActivityEvent),                     // "type": "activity"
    StrategyPhase { phase: String, strategy: String },  // "type": "strategy_phase"
    HitlPause { nonce: String, prompt: String },        // "type": "hitl_pause"
    Done { turn_id: Uuid },                             // "type": "done"
    Error { message: String },                          // "type": "error"
}
```

`CopilotActivityEvent` from `src/events/types.rs:415`. No new variants without a Phase 1 amendment.

## Security constraints

| Constraint | Implementation |
|-----------|---------------|
| UUID path validation | `Path<Uuid>` extractor rejects malformed UUIDs (CWE-20) |
| Body size | `RequestBodyLimit::new(32 * 1024)` on POST handlers |
| No session_id in query | session_id only in URL path (CWE-598) |
| Auth | `auth_middleware` on all 5 routes |
| HITL nonce | `handle.resume_registry` (confused-deputy prevention) |
