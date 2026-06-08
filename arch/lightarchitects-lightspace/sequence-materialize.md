# Sequence — Workspace materialisation choreography

Timeline budget: **≤1500ms** from operator submit to `phase=complete` event (G5 guarantee).

```mermaid
sequenceDiagram
  actor Op as Operator
  participant Lobby as ChatLobby.svelte
  participant WS as lightarchitects-webshell
  participant Redux as Lightspace reducer
  participant SSE as SSE handler
  participant UI as Svelte canvas

  Op->>Lobby: types intent + submits
  note over Lobby: T+0ms — intent captured

  Lobby->>WS: POST /api/lightspace/session\n{goal, strategy, budget}
  WS->>Redux: create session (new CanvasState)
  WS-->>Lobby: 201 {session_id: UUIDv7}
  note over Lobby: T+<200ms — session_id received

  Lobby->>SSE: GET /api/lightspace/session/:id/events (EventSource)
  SSE-->>UI: v1.agent.loop.step {phase: "begin"}
  note over UI: T+~200ms — rail collapses (phase=rail_collapsed)

  SSE-->>UI: v1.agent.loop.step {phase: "grid_revealed"}
  note over UI: T+~400ms — canvas grid appears

  SSE-->>UI: v1.agent.loop.step {phase: "drawer_revealed"}
  note over UI: T+~600ms — right drawer slides in

  loop Strategy loop running
    WS->>Redux: reduce(state, CanvasEvent::Card(card))
    Redux-->>WS: Ok(next_state)
    WS->>SSE: broadcast(LightspaceCard event)
    SSE-->>UI: card event → canvasAddCard()
    note over UI: T+600ms+ — cards stream in
  end

  WS->>Redux: reduce(state, CanvasEvent::Materialize{phase: complete})
  Redux-->>WS: Ok(next_state)
  WS->>SSE: broadcast(Materialize{phase: complete})
  SSE-->>UI: phase=complete event
  note over UI: T≤1500ms — G5 SLA met

  note over WS,Redux: Persistence: every event written to events.jsonl\nwith HMAC chain before SSE broadcast
```

## SSE reconnect / resume protocol

```
Client → GET /api/lightspace/session/:id/events?since_seq=N
  Running slot  → attach to broadcast::Sender; replay buffered events since seq N
  Paused slot   → replay events up to pause; emit v1.agent.loop.hitl
  Halted slot   → replay full event log from Arc<Vec<LightspaceEvent>> at recorded rate
```

## Branch-lane fork sequence (R2 — arXiv:2602.08199)

```mermaid
sequenceDiagram
  participant Engine as LoopRunner
  participant Reducer as Reducer
  participant SSE as SSE handler

  Engine->>SSE: BranchLane {lanes: [l1,l2,l3], fork_span_id}
  SSE->>Reducer: reduce(state, CanvasEvent::BranchLane{...})
  Reducer-->>SSE: Ok(next with 3 lane cards)
  SSE-->>UI: branchlane card → 3 lanes render

  Engine->>SSE: BranchLane {committed_lane_id: "l3"}
  SSE->>Reducer: reduce(state, CanvasEvent::BranchLane{committed_lane_id: Some("l3")})
  Reducer-->>SSE: Ok(next with lane l3 committed)
  SSE-->>UI: l3 committed → green highlight ≤200ms (S4 guarantee)
```
