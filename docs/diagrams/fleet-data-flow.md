# Fleet Data Flow — Sequence Diagram

> Canon XLI: Diagram-First. This diagram is a design input, not an output.
> The implementation in Phase 2 MUST conform to this flow.

```mermaid
sequenceDiagram
    autonumber
    participant CC as Claude Code<br/>(orchestrator session)
    participant JSONL as Session JSONL<br/>(~/.claude/projects/.../*.jsonl)
    participant Tailer as ClaudeJsonlTailer<br/>(fleet/tailer.rs)
    participant Tracker as FleetTracker<br/>(fleet/tracker.rs)
    participant Broadcaster as FleetBroadcaster<br/>(broadcast::Sender<FleetEvent>)
    participant SSE as SSE Handler<br/>(fleet_routes.rs GET /fleet)
    participant UI as FleetPanel<br/>(webshell-ui)

    Note over CC,JSONL: Agent tool_use spawned
    CC->>JSONL: Appends tool_use record<br/>{id, name:"Agent", input:{description, subagent_type,<br/>run_in_background, isolation, prompt}}

    Note over Tailer: Polling loop (100ms interval)
    Tailer->>JSONL: Read new lines since last offset
    JSONL-->>Tailer: Raw JSONL line(s)
    Tailer->>Tailer: Parse → filter tool_use where name=="Agent"
    Tailer->>Tailer: Deserialize input (prompt EXCLUDED)
    Tailer->>Tracker: TailerEvent::AgentSpawned { tool_use_id,<br/>subagent_type, description, run_in_background, isolation }

    Note over Tracker: State mutation
    Tracker->>Tracker: Create FleetSpan::new()<br/>(description truncated 200 chars,<br/>newlines stripped, null bytes stripped)
    Tracker->>Tracker: Insert into agents DashMap
    Tracker->>Broadcaster: FleetEvent::AgentSpawned(FleetSpan)

    Note over SSE,UI: Live subscribers
    SSE->>Broadcaster: Subscribe (broadcast::Receiver)
    Broadcaster-->>SSE: FleetEvent::AgentSpawned
    SSE->>SSE: Serialize to SSE frame
    SSE-->>UI: data: {"type":"agent_spawned", "node":{...}}
    UI->>UI: FleetPanel.addNode(node)

    Note over Tracker: Ticker (500ms interval)
    loop Every 500ms while agents running
        Tracker->>Tracker: Increment elapsed_ms for running agents
        Tracker->>Broadcaster: FleetEvent::Tick (progress batch)
        Broadcaster-->>SSE: FleetEvent::Tick
        SSE-->>UI: data: {"type":"agent_progress", "agent_id":"...", "elapsed_ms":...}
        UI->>UI: FleetPanel.updateElapsed()
    end

    Note over CC,JSONL: Agent completes
    CC->>JSONL: Appends tool_result record
    Tailer->>JSONL: Read new line
    JSONL-->>Tailer: tool_result record
    Tailer->>Tracker: TailerEvent::AgentCompleted { tool_use_id, exit_path, turns }
    Tracker->>Tracker: Update FleetSpan.status = Completed
    Tracker->>Tracker: Set turns, duration_ms, exit_path
    Tracker->>Broadcaster: FleetEvent::AgentCompleted { agent_id, exit_path, turns, duration_ms }
    Broadcaster-->>SSE: FleetEvent::AgentCompleted
    SSE-->>UI: data: {"type":"agent_completed", "agent_id":"...", ...}
    UI->>UI: FleetPanel.markCompleted()

    Note over SSE,UI: New subscriber (reconnect)
    UI->>SSE: GET /api/builds/{build_id}/fleet (reconnect)
    SSE->>Tracker: fleet_tracker.snapshot()
    Tracker-->>SSE: FleetSnapshot { nodes: Vec<FleetNode>, captured_at }
    SSE-->>UI: data: {"type":"snapshot", ...}
    UI->>UI: FleetPanel.replaceAll(snapshot.nodes)

    Note over SSE: Keepalive
    loop Every 30s
        SSE-->>UI: : keep-alive
    end
```

## Key design invariants this diagram encodes

1. **`prompt` is never in the flow** — Tailer reads it but immediately discards it during deserialization. FleetSpan constructor never receives it.
2. **FleetSpan::new() is the single sanitization point** — description normalization happens exactly here, before any storage or broadcast.
3. **Snapshot-on-reconnect** — the first event on any new SSE connection is always a full `FleetSnapshot`, preventing state divergence.
4. **Ticker is tracker-driven** — elapsed_ms is updated by the Tracker's internal ticker, not by the SSE handler. SSE handler is purely a subscriber.
5. **Broadcast fan-out** — multiple SSE connections receive the same events without per-subscriber state.
