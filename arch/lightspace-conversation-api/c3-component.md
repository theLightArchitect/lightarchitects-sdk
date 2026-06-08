# C3 Component — ConvSessionHandle + Routes + Lobby Wiring

```mermaid
C4Component
  title lightspace-conversation-api — C3 Component

  Container_Boundary(webshell, "lightarchitects-webshell (Axum)") {
    Component(routes, "conversation/routes.rs", "Axum handlers", "5 HTTP handlers: create_conversation_handler, conversation_stream_handler, conversation_turn_handler, conversation_interrupt_handler, conversation_end_handler")
    Component(session, "conversation/session.rs", "Core state", "ConvSessionHandle (session_id, event_tx, interrupt, resume_registry, inner). ConvSessionStore = DashMap<Uuid, Arc<ConvSessionHandle>>. spawn_eviction_task: TTL 1h, check every 5min.")
    Component(strategy_bridge, "conversation/strategy_bridge.rs", "Dispatch adapter", "should_route_to_strategy(msg) → Option<&str>. dispatch_conversation_strategy(handle, name, msg, state). dispatch_conversation_native(handle, msg, state).")
    Component(server_mod, "server/mod.rs", "AppState", "Holds conversation_store: Arc<ConvSessionStore>. Registers 5 routes. Spawns eviction task in new() and for_test().")
    Component(conv_event, "ConvSSEEvent (session.rs)", "SSE wire format", "Discriminated union: Activity(CopilotActivityEvent) | StrategyPhase{phase,strategy} | HitlPause{nonce,prompt} | Done{turn_id} | Error{message}")
  }

  Container_Boundary(webshell_ui, "lightarchitects-webshell-ui (SvelteKit)") {
    Component(conv_store, "lib/lightspace/conversation.svelte.ts", "Svelte store + API client", "convSession: writable<ConvSession|null>. createConversation(intent) → POST /api/conversation. sendMessage(id, msg) → POST /api/conversation/{id}. subscribeStream(id) → EventSource. endSession(id) → DELETE.")
    Component(lobby, "screens/lightspace/Lobby.svelte", "Demo entry point", "submit(): calls createConversation(intent), stores session_id in ls.sessionId, subscribes SSE, fires materialize animation ONLY after session_id received.")
  }

  Rel(lobby, conv_store, "Imports createConversation, subscribeStream", "TS import")
  Rel(conv_store, routes, "HTTP/SSE: POST /api/conversation, GET /api/conversation/{id}/stream, POST /api/conversation/{id}", "Fetch + EventSource")
  Rel(routes, session, "state.conversation_store.get(&id); Arc<ConvSessionHandle>", "Rust inprocess")
  Rel(routes, strategy_bridge, "dispatch_conversation_turn(handle, msg, state)", "Rust inprocess (tokio::spawn)")
  Rel(strategy_bridge, session, "handle.event_tx.send(ConvSSEEvent::...)", "broadcast::Sender")
  Rel(session, conv_store, "SSE frames: ConvSSEEvent serialized as JSON", "broadcast channel → SSE stream")
  Rel(server_mod, session, "Owns ConvSessionStore; spawns eviction task", "Arc<ConvSessionStore>")
```

## Component responsibilities (summary)

| Component | Owns | Does NOT own |
|-----------|------|-------------|
| `routes.rs` | HTTP request parsing, 404/401 response, tokio::spawn dispatch | Turn execution logic |
| `session.rs` | Session lifecycle, event broadcast, TTL eviction | HTTP concerns |
| `strategy_bridge.rs` | Message routing (prefix → strategy or native) | Session creation/cleanup |
| `conversation.svelte.ts` | API client state, SSE subscription | UI animation |
| `Lobby.svelte` | Demo UX: loading state, error toast, materialize gate | API client implementation |
