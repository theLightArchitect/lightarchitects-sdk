# PROPOSAL — FleetNode / FleetSpan extensions for build-progress join

Companion to:
- `arch/cockpit-d2-build-progress.contract.proposal.yaml`
- `arch/cockpit-d2-manifest-extension.proposal.yaml`

## Why

The D2 cockpit drawer needs to render **agent badges per task row** — the "▶ T" pill that pulses when CORSO wave-2 TEST is actively focused on `t3.2.1`. That join requires the fleet entry to carry the `task_id` it's working on.

Today, `FleetNode` (`lightarchitects/src/fleet/tracker.rs:25`) carries `agent_id, agent_type, description, parent_agent_id, worktree_path, status, turns, elapsed_ms, exit_path`. It does **not** carry `build_codename`, `wave_id`, `task_id`, or `focus_target_fn`.

Without these fields, the gateway handler synthesizing `/v1/platform/builds/{codename}/progress` can only return agents under the build root — not under specific waves or tasks. The cockpit's drilldown contract collapses; the operator can't tell which task an agent is inside without reading description text.

## What changes

### `lightarchitects/src/fleet/span.rs` — `FleetSpan` struct

Add four optional fields, all defaulted to `None`:

```rust
// EXISTING fields elided for brevity — only additions shown.
#[derive(Debug, Clone)]
pub struct FleetSpan {
    // ... existing fields ...

    /// Build codename this agent is working on (cross-ref to manifest.yaml).
    /// `None` for agents not invoked via /BUILD (e.g. ad-hoc /SCRUM, /RESEARCH).
    pub build_codename: Option<String>,

    /// Wave ID within the build (cross-ref to manifest.phases[i].waves[j].id).
    /// `None` when the agent is build-scoped but not wave-bound (rare).
    pub wave_id: Option<String>,

    /// Task ID within the wave (cross-ref to manifest.phases[i].waves[j].tasks[k].id).
    /// `None` when the agent is wave-scoped but not yet focused on a specific task.
    pub task_id: Option<String>,

    /// Symbol focus — for tasks targeting a specific function/struct.
    /// E.g. "fn handle_message" when an agent is working on coverage for that fn.
    pub focus_target_fn: Option<String>,
}
```

### `lightarchitects/src/fleet/tracker.rs` — `FleetNode` projection

`FleetNode` is the serializable view of `FleetSpan`. Mirror the four additions there with `#[serde(skip_serializing_if = "Option::is_none")]` so existing consumers don't see new keys until the producer side actually populates them.

```rust
#[derive(Debug, Clone, Serialize)]
pub struct FleetNode {
    // ... existing fields ...

    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_codename: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_target_fn: Option<String>,
}

impl From<&FleetSpan> for FleetNode {
    fn from(span: &FleetSpan) -> Self {
        Self {
            // ... existing ...
            build_codename:   span.build_codename.clone(),
            wave_id:          span.wave_id.clone(),
            task_id:          span.task_id.clone(),
            focus_target_fn:  span.focus_target_fn.clone(),
        }
    }
}
```

### Producer side — `lightarchitects/src/fleet/jsonl.rs` (the tailer)

The tailer reads Claude Code session JSONL files and produces `FleetSpan` entries. To populate the new fields, the JSONL events need to carry the build context.

**Option A — JSONL emitter side:**
The `/BUILD` wave-dispatcher emits an enriched Agent-tool-call event whenever it spawns a worker:

```jsonl
{
  "type":"agent_spawn",
  "agent_id":"corso-w3.2-test-001",
  "subagent_type":"engineer",
  "description":"cover handle_message error branches",
  "wave_context":{
    "build_codename":"webshell-copilot-providers",
    "wave_id":"w3.2",
    "task_id":"t3.2.1",
    "focus_target_fn":"fn handle_message"
  }
}
```

The tailer parses `wave_context` (if present) and writes the four fields onto the `FleetSpan`.

**Option B — registration table:**
A separate `~/.lightarchitects/state/wave_context.toml` maps `agent_id → (codename, wave_id, task_id)`. The tailer consults it on every spawn event.

→ **Recommend Option A.** Single-source; no synchronisation problem; survives tailer restart. Option B is fragile (race conditions on agent_id reuse).

### AYIN HTTP surface — `lightarchitects-gateway` or `AYIN-DEV`

Today the gateway exposes `/v1/platform/agents/{sibling}` and `/v1/platform/agents/{sibling}/strands`. Neither returns the live fleet (these are static metadata).

The live fleet is served by AYIN at `:3742/api/fleet` (HTTP-only sibling). The cockpit handler for `/v1/platform/builds/{codename}/progress` must:

1. Read manifest.yaml → get `phases[].waves[].tasks[]`.
2. GET `http://127.0.0.1:3742/api/fleet` → get all running agents.
3. Filter fleet entries where `node.build_codename === codename`.
4. Join each filtered entry into the matching `wave.agents[]` array by `node.wave_id`.

**Failure mode:** AYIN unreachable. The contract's `fleet_required` query param controls behavior:
- `fleet_required=false` (default): set `agents[].state = "unknown"` and continue.
- `fleet_required=true`: return 503 `E_FLEET_UNAVAILABLE`.

## Backwards compatibility

All four fields are `Option<T>` and serialize-skipped when `None`. Existing fleet consumers (AYIN dashboard at `:3742`, any other readers) see no schema change until the producer populates them. The cockpit handler treats `None` as "agent isn't bound to a wave" and surfaces it under the build root rather than a wave.

## Conformance test

Add to `lightarchitects/src/fleet/tracker.rs` tests:

```rust
#[tokio::test]
async fn fleet_node_carries_wave_context_when_present() {
    let mut span = FleetSpan::new(
        "corso-w3.2-test-001".into(),
        "engineer".into(),
        "cover handle_message branches".into(),
    );
    span.build_codename = Some("webshell-copilot-providers".into());
    span.wave_id = Some("w3.2".into());
    span.task_id = Some("t3.2.1".into());
    span.focus_target_fn = Some("fn handle_message".into());

    let node = FleetNode::from(&span);
    assert_eq!(node.build_codename.as_deref(), Some("webshell-copilot-providers"));
    assert_eq!(node.wave_id.as_deref(), Some("w3.2"));
    assert_eq!(node.task_id.as_deref(), Some("t3.2.1"));
    assert_eq!(node.focus_target_fn.as_deref(), Some("fn handle_message"));

    // Backwards compat: serialization skips when None.
    let mut bare = FleetSpan::new("ad-hoc-001".into(), "researcher".into(), "scratch".into());
    let bare_node = FleetNode::from(&bare);
    let json = serde_json::to_string(&bare_node).unwrap();
    assert!(!json.contains("build_codename"));
    assert!(!json.contains("wave_id"));
}
```

## Open questions

- **F-1**: Should `focus_target_fn` accept structured `{ kind: 'fn'|'struct'|'trait'|'mod', name: '...', file: '...' }` instead of a string? The cockpit currently displays it as bare text. For symbol-level d3 drill, structured is better. **Recommendation**: keep string for v0.1.0; refactor to structured in v0.2.0 alongside d3 symbol-drill plumbing.

- **F-2**: When an agent finishes a task and moves to the next, is that a state change on the same `FleetSpan` (mutate `task_id`) or a new span entry? **Recommendation**: same span, mutable `task_id` — the agent identity is stable; only its focus shifts. The state-machine update is `tracker.rs::agent_focused_on(span_id, new_task_id)`.

- **F-3**: How does focus get cleared when no task is active? Set `task_id = None`, or `task_id = Some("idle")`? **Recommendation**: `None`. "idle" introduces a sentinel that contracts have to special-case.

## Files touched (when this proposal is implemented)

| File | Change |
|------|--------|
| `lightarchitects/src/fleet/span.rs` | Add 4 fields to `FleetSpan`. |
| `lightarchitects/src/fleet/tracker.rs` | Mirror fields on `FleetNode` + update `From<&FleetSpan>` + add `agent_focused_on(...)` method. |
| `lightarchitects/src/fleet/jsonl.rs` | Parse `wave_context` from JSONL events; populate fields. |
| `lightarchitects-gateway/src/http/routes/builds.rs` | (new file) Implements `/v1/platform/builds/{codename}/progress`. Joins manifest + fleet. |
| `lightarchitects/src/lightsquad/wave_dispatcher.rs` | Emit `wave_context` in agent_spawn events. |
| Test fixture | `$HELIX/corso/builds/test-fixture-build/manifest.yaml` for the contract's conformance test. |
