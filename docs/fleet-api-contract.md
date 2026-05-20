# Fleet API Contract

> **Status**: DRAFT — Phase 1 research artifact. Subject to HITL review before Phase 2 implementation.  
> **Version**: 1.0.0  
> **Date**: 2026-05-18  
> **Ephemeral**: This file will be deleted at Phase 6 after helix enrichment. The contract lives in the helix entry.

---

## Overview

The Fleet API provides real-time visibility into agent sub-processes spawned by a Claude Code orchestrator session. It exposes two endpoints:

1. **SSE stream** — push-based, live updates as agents spawn/run/complete
2. **Snapshot** — pull-based, point-in-time state for reconnect/render

Both endpoints scope to a specific `build_id` (the orchestrating build's UUID). Data is read from the Claude Code JSONL tailer and served via an in-memory `FleetTracker`.

---

## Authentication

All endpoints require a valid Bearer token in the `Authorization` header.

```
Authorization: Bearer <token>
```

- **Missing or invalid token**: `401 Unauthorized`
- Token validation via the existing `AuthGuard` extractor (constant-time comparison).
- The `X-LA-Notify-Token` header is NOT accepted on these endpoints (machine-only, excluded by design — CWE-306).

---

## Endpoint 1: SSE Fleet Stream

### Request

```
GET /api/builds/{build_id}/fleet
Authorization: Bearer <token>
Accept: text/event-stream
```

**Path parameters**:

| Parameter | Type | Description |
|-----------|------|-------------|
| `build_id` | UUID string | The build identifier. Must be a valid UUID v4. |

**Query parameters**: None.

### Success Response: `200 OK`

```
Content-Type: text/event-stream
Cache-Control: no-cache
X-Accel-Buffering: no
```

The response body is an indefinitely-open SSE stream. Events are newline-delimited per the SSE specification (RFC 8895).

#### First event: Snapshot

Immediately after connection (before any other events), the server sends a full `FleetSnapshot`. This enables the client to render current state without replaying history.

```
data: {"type":"snapshot","nodes":[...],"captured_at":"2026-05-18T14:32:00.000Z"}

```

#### Subsequent events

Events are delivered in chronological order as state changes occur. Each SSE frame is a single JSON object on the `data:` line, followed by a blank line.

#### Keepalive

Every 30 seconds with no other events, the server emits a comment to prevent proxy/load-balancer timeout:

```
: keep-alive

```

### Error Responses

| Status | Condition |
|--------|----------|
| `401 Unauthorized` | Missing or invalid Bearer token |
| `404 Not Found` | `build_id` does not correspond to a known build |
| `429 Too Many Requests` | Concurrent SSE connection cap (100 per `build_id`) exceeded |

**429 response headers**:
```
X-Webshell-Reason: fleet-sse-cap
```

### Connection lifecycle

1. Client connects → server increments atomic connection counter.
2. Server emits snapshot (first event).
3. Server subscribes to `FleetBroadcaster` and forwards events to the client.
4. Client disconnects (or server detects broken pipe) → server decrements atomic counter via RAII guard.

The cap of 100 concurrent connections is per `build_id`. Multiple builds have independent counters.

---

## Endpoint 2: Fleet Snapshot

### Request

```
GET /api/builds/{build_id}/fleet/snapshot
Authorization: Bearer <token>
```

**Path parameters**:

| Parameter | Type | Description |
|-----------|------|-------------|
| `build_id` | UUID string | The build identifier. |

### Success Response: `200 OK`

```
Content-Type: application/json
```

Response body is a `FleetSnapshot` JSON object.

**Performance target**: Response time `<200ms` under normal load. The snapshot is read from an in-memory `DashMap<String, FleetSpan>` — no database query.

### Error Responses

| Status | Condition |
|--------|----------|
| `401 Unauthorized` | Missing or invalid Bearer token |
| `404 Not Found` | `build_id` not found |

---

## Data Types

### `FleetNode`

The external representation of a `FleetSpan`. Serialized as JSON in both SSE events and snapshot responses.

```typescript
interface FleetNode {
  // Identity
  agent_id: string;           // tool_use_id from Claude Code JSONL (e.g., "toolu_01ALq...")
  agent_type: string;         // Normalized subagent_type. Prefix "lightarchitects:" stripped.
                              // Examples: "engineer", "quality", "ops", "security", "knowledge",
                              //           "testing", "researcher", "Explore", "feature-dev:code-reviewer"
                              // Unknown/absent subagent_type: passed through as-is.

  // Display
  description: string;        // Truncated at 200 chars. Newlines (\n, \r) replaced with space.
                              // Null bytes stripped. Sanitized at FleetSpan::new().

  // Topology
  parent_agent_id: string | null; // agent_id of the spawning agent. null for root-level agents.
  worktree_path: string | null;   // Absolute path to the worktree (if isolation == "worktree").
                                  // null if no isolation or non-worktree isolation.

  // Configuration
  run_in_background: boolean; // Whether spawned with run_in_background: true.

  // State machine
  status: "queued" | "running" | "completed" | "failed" | "stalled";

  // Progress
  turns: number;              // u64. 0 while running. Populated only at completion from tool_result.
  elapsed_ms: number;         // u64. Timer-driven, updated every 500ms while status == "running".
                              // Frozen at final value after completion/failure/stall.

  // Completion
  exit_path: "completed" | "error" | "watchdog_stall" | null;
                              // null while agent is running or queued.
}
```

**Invariants**:
- `description` length is always `<= 200` characters (enforced at `FleetSpan::new()`).
- `description` contains no `\n`, `\r`, or null (`\0`) characters.
- `agent_id` is always a non-empty string (the `tool_use_id` from JSONL).
- `prompt` from the JSONL input is NEVER present in `FleetNode`. This is enforced at the struct level.
- `turns` is `0` for all statuses except `completed` and `failed`.
- `elapsed_ms` is `0` when status is `queued`.
- `exit_path` is non-null only when status is `completed`, `failed`, or `stalled`.

### `FleetSnapshot`

```typescript
interface FleetSnapshot {
  nodes: FleetNode[];
  captured_at: string;  // ISO 8601 UTC timestamp. e.g., "2026-05-18T14:32:00.000Z"
}
```

`nodes` contains ALL known agents for the build, regardless of status. Ordering is insertion order (spawn order).

### `FleetEvent` (SSE event payload)

All SSE events share a discriminated union shape with a `"type"` field.

#### `snapshot` event

Emitted as the first event on connection and on reconnect.

```typescript
interface SnapshotEvent {
  type: "snapshot";
  nodes: FleetNode[];
  captured_at: string;
}
```

#### `agent_spawned` event

Emitted when a new Agent tool_use record is detected in the JSONL stream.

```typescript
interface AgentSpawnedEvent {
  type: "agent_spawned";
  node: FleetNode;  // Full FleetNode for the newly-spawned agent. status == "running".
}
```

#### `agent_progress` event

Emitted every 500ms for each running agent (timer-driven, not JSONL-driven).

```typescript
interface AgentProgressEvent {
  type: "agent_progress";
  agent_id: string;
  elapsed_ms: number;  // Current elapsed time in milliseconds.
}
```

**Note**: `agent_progress` events are omitted if no agents are currently running (no unnecessary keepalive traffic).

#### `agent_completed` event

Emitted when a tool_result record is detected for a previously-spawned agent.

```typescript
interface AgentCompletedEvent {
  type: "agent_completed";
  agent_id: string;
  exit_path: "completed" | "error" | "watchdog_stall";
  turns: number;       // Total turn count from the tool_result.
  duration_ms: number; // Total duration in milliseconds.
}
```

---

## SSE Wire Format Examples

### Connection sequence (complete example)

```
# Connection established. Server emits snapshot immediately.
data: {"type":"snapshot","nodes":[{"agent_id":"toolu_01ALq","agent_type":"engineer","description":"Sprint 4-A engineer: implement vital-streaming-prism","parent_agent_id":null,"worktree_path":"/Users/kft/lightarchitects/worktrees/vital-streaming-prism","run_in_background":true,"status":"running","turns":0,"elapsed_ms":4200,"exit_path":null}],"captured_at":"2026-05-18T14:32:00.000Z"}

# 500ms later — progress tick
data: {"type":"agent_progress","agent_id":"toolu_01ALq","elapsed_ms":4700}

# New agent spawned
data: {"type":"agent_spawned","node":{"agent_id":"toolu_019h8","agent_type":"ops","description":"Sprint 4-A ops: DevOps assessment","parent_agent_id":null,"worktree_path":null,"run_in_background":true,"status":"running","turns":0,"elapsed_ms":0,"exit_path":null}}

# 30s with no events — keepalive
: keep-alive

# Agent completes
data: {"type":"agent_completed","agent_id":"toolu_01ALq","exit_path":"completed","turns":12,"duration_ms":183400}

```

### 429 response

```
HTTP/1.1 429 Too Many Requests
X-Webshell-Reason: fleet-sse-cap
Content-Type: application/json

{"error":"fleet SSE connection cap exceeded","max_connections":100}
```

---

## Rust Type Mapping

For Phase 2 implementation reference:

| API field | Rust type | Notes |
|-----------|-----------|-------|
| `agent_id` | `String` | From JSONL `tool_use.id` |
| `agent_type` | `String` | Normalized from `subagent_type` |
| `description` | `String` | Sanitized in `FleetSpan::new()` |
| `parent_agent_id` | `Option<String>` | `null` for root agents |
| `worktree_path` | `Option<String>` | Serialized as string on Unix |
| `run_in_background` | `bool` | From JSONL field |
| `status` | `FleetStatus` enum | `#[serde(rename_all = "snake_case")]` |
| `turns` | `u64` | |
| `elapsed_ms` | `u64` | |
| `exit_path` | `Option<ExitPath>` | `#[serde(rename_all = "snake_case")]` |
| `captured_at` (snapshot) | `String` | RFC 3339 UTC from `chrono::Utc::now()` |

**`FleetStatus` enum** (Rust):
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Stalled,
}
```

**`ExitPath` enum** (Rust):
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitPath {
    Completed,
    Error,
    WatchdogStall,
}
```

---

## Security Invariants (normative)

These invariants MUST be preserved by the implementation. Violation is a blocking security defect.

1. **S1**: `prompt` field from JSONL input MUST NOT appear in any `FleetNode`, `FleetSnapshot`, or `FleetEvent` serialization. Enforced at struct level (field absent from `FleetSpan`).
2. **S2**: All fleet endpoints MUST require a valid Bearer token. `401` on missing/invalid. No anonymous access.
3. **S3**: JSONL file path MUST be validated against `$HOME` prefix before `File::open`. `None` returned on traversal attempt.
4. **S4**: `description` field MUST be truncated to 200 characters and stripped of `\n`, `\r`, and null bytes. Enforced in `FleetSpan::new()`.
5. **S5**: SSE connection cap MUST be enforced with RAII guard to prevent cap underflow on disconnect. 429 returned on cap breach.

---

## Open Questions (for HITL review)

1. **`parent_agent_id` source**: The JSONL records do not contain a parent agent ID. This must be inferred from the session structure (the spawning agent's `tool_use_id` is in the parent's content array). Implementation approach: `FleetTracker` tracks the current "active" agent and assigns parent from context. Confirm this approach is correct.

2. **`worktree_path` source**: The `isolation` field in JSONL is `"worktree"` (a string), not a path. The actual worktree path is stored elsewhere (build manifest? `session_cwd.rs`?). Implementation approach: look up worktree path from `AppState.build` record keyed on `agent_id`. Confirm the lookup source.

3. **Watchdog stall detection**: The `stalled` status and `watchdog_stall` exit_path require a timeout mechanism. The plan mentions watchdog but does not specify the stall timeout duration. Suggested: 30 minutes of no activity (no JSONL progress) → mark as stalled. Confirm timeout value.

4. **`turns` from JSONL**: Tool_result records do not directly contain a turn count in the live records sampled. Turns may need to be inferred from counting tool_use records within the sub-agent's sub-session JSONL. Confirm source of `turns` data.

5. **Fleet tracker lifecycle**: When does `FleetTracker` start/stop? At build start? At first agent spawn? Confirm the trigger for `ClaudeJsonlTailer` initialization in relation to the build lifecycle in `AppState`.
