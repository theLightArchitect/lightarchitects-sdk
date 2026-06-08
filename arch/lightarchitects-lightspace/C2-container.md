# C2 — Container: Lightspace runtime deployment

```mermaid
graph TB
  subgraph "Browser (Svelte/TS)"
    UI_Lobby[ChatLobby\nd0 entry]
    UI_Rail[CopilotRail\nleft collapsible]
    UI_Canvas[LightspaceCanvas\nbento grid]
    UI_Drawer[LightspaceDrawer\nright files]
    UI_Cards[Card components\n11 kinds]
    UI_SSE[sse.ts\nEventSource client]
  end

  subgraph "lightarchitects-webshell (Rust, axum)"
    SSE_H[lightspace_sse_handler\nbroadcast::Sender]
    Snap[GET /api/lightspace/snapshot]
    Replay[GET /api/lightspace/replay/:id/stream]
    Persist[persist.rs\nNDJSON + HMAC chain]
    AppState[AppState.lightspace_engines\nDashMap per session]
    Auth[Bearer token\nmiddleware]
  end

  subgraph "lightarchitects-lightspace (Rust crate, SDK)"
    Reducer[Lightspace struct\nimpl Reducer trait]
    Types[CanvasEvent enum\nCanvasState struct]
    Snapshot_sd[Snapshot ser/de]
    Gates[gates.rs\nauto-re-eval]
    Contra[contradictions.rs\ncycle detect]
  end

  subgraph "Copilot loop"
    Copilot[copilot/mod.rs\nproposes cards]
  end

  subgraph "lightshell (CLI/TUI)"
    TUI_R[lightspace_tui.rs\nratatui renderer]
    Replay_R[replay reader\nfrom Disk]
  end

  UI_Lobby --> UI_Rail
  UI_Rail  --> UI_SSE
  UI_Canvas --> UI_SSE
  UI_Drawer --> UI_SSE
  UI_Cards  --> UI_SSE
  UI_SSE   -- SSE --> SSE_H
  UI_SSE   -- GET --> Snap
  UI_SSE   -- GET stream --> Replay
  SSE_H   -- reduce --> AppState
  AppState -- reduce --> Reducer
  Reducer -- uses --> Types
  Reducer -- uses --> Gates
  Reducer -- uses --> Contra
  Reducer -- snapshot --> Snapshot_sd
  SSE_H   -- write --> Persist
  Persist -- replay --> Replay
  Copilot -- POST cards --> SSE_H
  TUI_R   -- reduce --> Reducer
  Replay_R -- read --> Persist
```

**Lock invariant — Wave 2 shape**: `AppState.lightspace_engines` is initially `DashMap<SessionId, Arc<RwLock<Lightspace>>>`. Snapshot endpoint takes **read guard**; reducer apply path takes **write guard**. No nested guards within `reduce()`.

**Wave 2b extension** (Absorbed Plan Decisions — session_registry.rs): The entry is extended to a unified `LightspaceSessionState` struct that holds both the `Lightspace` reducer state AND the `SessionSlot` enum (Running/Paused/Halted). This supports SSE reconnect replay without losing reducer state on browser refresh. The lock invariant remains unchanged — `Arc<RwLock<LightspaceSessionState>>`.
