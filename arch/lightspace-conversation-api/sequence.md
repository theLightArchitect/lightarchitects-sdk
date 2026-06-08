# Sequence — POST /api/conversation → SSE Stream → Lobby Materialize

Two separate flows: (1) Lobby submit creates session + subscribes; (2) user sends first message as a separate action.

## Flow A: Lobby submit (demo action — creates session + materializes workspace)

```mermaid
sequenceDiagram
  autonumber
  actor User as Operator (LightSpace demo)
  participant Lobby as Lobby.svelte
  participant ConvStore as conversation.svelte.ts
  participant Backend as lightarchitects-webshell (Axum)
  participant Session as ConvSessionHandle

  Note over Lobby: submit() fires — lobbyInput = "react audit auth"

  Lobby->>ConvStore: createConversation({ intent: "react audit auth" })
  ConvStore->>Backend: POST /api/conversation\n{ intent: "react audit auth" }
  Backend->>Session: ConvSessionHandle::new(session_id=UUID, event_tx=broadcast::channel(256))
  Backend->>Backend: conversation_store.insert(session_id, Arc::new(handle))
  Backend-->>ConvStore: 200 { session_id: "550e8400-...", stream_url: "..." }
  ConvStore-->>Lobby: returns { session_id }

  Note over Lobby: ls.sessionId = session_id (state.svelte.ts:36 — direct $state write)

  Lobby->>ConvStore: subscribeStream(session_id)
  ConvStore->>Backend: GET /api/conversation/{id}/stream (SSE connection opened)
  Backend->>Session: event_tx.subscribe() → broadcast::Receiver
  Backend-->>ConvStore: 200 text/event-stream headers (connection held open)

  Lobby->>Lobby: ls.exitLobby() — CSS materialize animation (fires AFTER session_id received)
  Lobby->>Lobby: setMatPhase sequence: begin → rail_collapsed → grid_revealed → ...\n(state.svelte.ts:91 setMatPhase; MatPhaseId from types.ts:29)

  Note over Lobby,Backend: SSE stream open — workspace materialised around real session
```

## Flow B: User sends first message (separate action after workspace materialises)

```mermaid
sequenceDiagram
  autonumber
  actor User as Operator
  participant Lobby as Lobby.svelte
  participant ConvStore as conversation.svelte.ts
  participant Backend as lightarchitects-webshell (Axum)
  participant Session as ConvSessionHandle
  participant Bridge as strategy_bridge.rs
  participant Lib as lightarchitects::ConversationSession + StrategyRegistry

  User->>ConvStore: sendMessage(session_id, "/react audit auth")
  ConvStore->>Backend: POST /api/conversation/{id}\n{ message: "/react audit auth" }
  Backend->>Backend: handle.last_active.lock().unwrap().clone_from(&Instant::now())
  Backend->>Bridge: should_route_to_strategy("/react audit auth")
  Bridge-->>Backend: Some("react")
  Backend->>Bridge: tokio::spawn → dispatch_conversation_strategy(handle, "react", msg, state)

  Bridge->>Lib: StrategyRegistry::lookup("react") → ReActStrategy
  Bridge->>Lib: strategy.run(ConversationSession, interrupt_flag)

  Backend-->>User: 202 Accepted (dispatch async; events come via SSE)

  Note over Lib,Session: DEMO MONEY MOMENT — first event within ≤1s

  Lib->>Session: event_tx.send(ConvSSEEvent::StrategyPhase { phase: "analyze", strategy: "react" })
  Session-->>Backend: SSE frame: {"type":"strategy_phase","phase":"analyze","strategy":"react"}
  Backend-->>ConvStore: SSE event via EventSource
  ConvStore-->>Lobby: convSession.events updated → canvas card appears

  Lib->>Session: event_tx.send(ConvSSEEvent::Activity(CopilotActivityEvent { ... }))
  Note over Lib: CopilotActivityEvent from events/types.rs:415
  Session-->>Backend: SSE frame: {"type":"activity","text":"...","kind":"reasoning"}
  Backend-->>ConvStore: SSE event
  ConvStore-->>Lobby: additional canvas cards stream in

  Lib->>Session: event_tx.send(ConvSSEEvent::Done { turn_id: Uuid::new_v4() })
  Session-->>Backend: SSE frame: {"type":"done","turn_id":"..."}
  ConvStore->>ConvStore: convSession.status = "idle"
```

## Error path (LiteLLM not configured — demo resilience)

```mermaid
sequenceDiagram
  participant Bridge as strategy_bridge.rs
  participant Session as ConvSessionHandle (event_tx)
  participant Lobby as Lobby.svelte

  Bridge->>Bridge: LitellmConfig::build_provider() → Err("LiteLLM not configured")
  Bridge->>Session: event_tx.send(ConvSSEEvent::Error { message: "LLM not configured — check Settings" })
  Session-->>Lobby: SSE error event
  Lobby->>Lobby: show error toast (NOT crash — demo resilience)
  Lobby->>Lobby: convSession.status = "error"
  Note over Lobby: submit button re-enabled; user can retry
```
