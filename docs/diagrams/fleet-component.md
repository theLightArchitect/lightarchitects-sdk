# Fleet Component Dependency Graph

> Canon XLI: Diagram-First. This diagram is a design input, not an output.
> Component boundaries and dependency directions MUST be preserved in implementation.

```mermaid
graph LR
    subgraph SDK ["lightarchitects SDK crate"]
        SPAN["fleet/span.rs<br/>FleetSpan, FleetStatus, ExitPath"]
        TRACKER["fleet/tracker.rs<br/>FleetTracker, FleetBroadcaster"]
        TAILER["fleet/tailer.rs<br/>ClaudeJsonlTailer, TailerEvent"]
        SESSIONCWD["session_cwd.rs<br/>find_jsonl_for_session()"]
        MOD["fleet/mod.rs<br/>pub re-exports"]
    end

    subgraph WEBSHELL ["lightarchitects-webshell (Rust backend)"]
        FLEETROUTES["fleet_routes.rs<br/>GET /api/builds/:id/fleet<br/>GET /api/builds/:id/fleet/snapshot"]
        APPSTATE["AppState<br/>fleet_tracker: Arc<FleetTracker>"]
        AUTHGUARD["AuthGuard<br/>Bearer token validation"]
        SSEPATTERN["agent/sse.rs (reuse)<br/>SseGuard + AtomicUsize pattern"]
    end

    subgraph WEBSHELL_EVENTS ["lightarchitects-webshell events"]
        WEBEVENT["events/types.rs<br/>WebEvent enum<br/>+ AgentFleetUpdate(FleetSnapshot)"]
    end

    subgraph WEBSHELL_UI ["lightarchitects-webshell-ui (Svelte)"]
        FLEETPANEL["FleetPanel.svelte<br/>Tree visualization"]
        FLEETNODE_TS["lib/types.ts<br/>FleetNode, FleetEvent, FleetSnapshot"]
        SSESTORE["lib/stores/sse.ts<br/>SSE connection + event dispatch"]
    end

    subgraph STORAGE ["Runtime state"]
        JSONL["~/.claude/projects/.../session.jsonl<br/>(Claude Code JSONL file)"]
        DASHMAP["DashMap<agent_id, FleetSpan><br/>(in-memory, per FleetTracker)"]
        BROADCAST["broadcast::Sender<FleetEvent><br/>(tokio, fan-out to N subscribers)"]
    end

    %% SDK internal deps
    TAILER --> SESSIONCWD
    TAILER --> SPAN
    TRACKER --> SPAN
    TRACKER --> TAILER
    TRACKER --> BROADCAST
    TRACKER --> DASHMAP
    MOD --> SPAN
    MOD --> TRACKER
    MOD --> TAILER

    %% SDK reads filesystem
    TAILER -.->|poll 100ms| JSONL

    %% Webshell backend deps on SDK
    FLEETROUTES --> MOD
    FLEETROUTES --> SSEPATTERN
    APPSTATE --> TRACKER
    FLEETROUTES --> APPSTATE
    FLEETROUTES --> AUTHGUARD
    WEBEVENT -.->|future: GlobalWebEvent| BROADCAST

    %% Webshell UI deps
    FLEETPANEL --> FLEETNODE_TS
    FLEETPANEL --> SSESTORE
    SSESTORE -.->|SSE HTTP| FLEETROUTES

    %% Style
    style SDK fill:#1a1a2e,stroke:#4a90d9,color:#e0e0e0
    style WEBSHELL fill:#162032,stroke:#4a90d9,color:#e0e0e0
    style WEBSHELL_EVENTS fill:#162032,stroke:#4a90d9,color:#e0e0e0
    style WEBSHELL_UI fill:#1a2e1a,stroke:#4a9d4a,color:#e0e0e0
    style STORAGE fill:#2e1a1a,stroke:#9d4a4a,color:#e0e0e0
```

## Dependency direction invariants

| Rule | Rationale |
|------|----------|
| SDK fleet module has NO dependency on webshell | SDK is consumed by webshell, not the reverse. Fleet types must be publishable standalone. |
| `fleet_routes.rs` depends on SDK via `lightarchitects` crate path dep | Follows existing workspace pattern (CORSO/EVA path dep to soul). |
| `WebEvent::AgentFleetUpdate` is additive | Does not break existing WebEvent consumers. Serde tag-based dispatch. |
| `find_jsonl_for_session` lives in `session_cwd.rs` | Centralizes JSONL path derivation logic; reuses existing HOME-prefix validation pattern. |
| `FleetTracker` is `Arc<FleetTracker>` in `AppState` | Shared across route handlers without locking the full AppState. |
| UI `FleetPanel` never reads JSONL directly | All data flows through the SSE endpoint. UI is display-only. |

## Crate boundary summary

```
lightarchitects (SDK)
  └── fleet/       [NEW — Phase 2]
       ├── mod.rs
       ├── span.rs     FleetSpan, FleetStatus, ExitPath
       ├── tracker.rs  FleetTracker (Arc-safe), FleetBroadcaster
       └── tailer.rs   ClaudeJsonlTailer (tokio task)

lightarchitects-webshell (backend)
  └── src/
       ├── session_cwd.rs    [MODIFY — add find_jsonl_for_session]
       ├── events/types.rs   [MODIFY — add AgentFleetUpdate variant]
       └── fleet_routes.rs   [NEW — Phase 3]

lightarchitects-webshell-ui (frontend)
  └── src/
       ├── lib/types.ts         [MODIFY — add FleetNode, FleetEvent, FleetSnapshot]
       ├── lib/stores/sse.ts    [MODIFY — handle AgentFleetUpdate]
       └── lib/components/
            └── FleetPanel.svelte  [NEW — Phase 4]
```
