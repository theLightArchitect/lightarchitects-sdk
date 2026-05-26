# CLAUDE.md — lightarchitects-gateway

MCP gateway binary. Stdio JSON-RPC server + Arena HTTP + Conductor task queue.

## Build

```bash
# workspace-excluded; build directly:
cd lightarchitects-gateway && cargo build --release
# or: cargo build --release -p lightarchitects-gateway (from workspace root after temporarily un-excluding)
```

## AYIN Span Instrumentation (copilot-ayin-instrumentation — shipped 2026-05-26)

Spans are written via `GatewaySpanContext` (task-local) + `write_span_to_disk` (atomic tmp→rename + F_FULLFSYNC).

### Key files

| File | Purpose |
|------|---------|
| `src/span_context.rs` | `GatewaySpanContext`, `SPAN_CTX` task_local, `spawn_with_span_context`, `write_span_to_disk` |
| `src/server.rs` | `emit_tool_dispatch_span` — writes `gateway.tool.dispatch` span per MCP tool call |
| `src/llm.rs` | Writes `llm.call` span with `parent_id` from `current_span_ctx()` |
| `src/http/middleware/ayin_trace.rs` | Writes `platform.http.request` span per Arena HTTP request |
| `src/agent_stream/strategy.rs` | Writes `gateway.session.start` span at strategy entry |
| `.cargo/ci-denylist.sh` | Enforces `spawn_with_span_context` usage — never bare `tokio::spawn` |

### Rules

- Use `spawn_with_span_context(async move { ... })` NOT `tokio::spawn(...)` for any async span write
- Context defaults to `GatewaySpanContext { session_id: None, parent_id: None }` outside a `with_span_context` scope
- Spans >64KB are silently dropped (eviction-attack mitigation); oversized spans return `Ok(())`
- Trace files land at `~/lightarchitects/soul/helix/ayin/traces/gateway/<YYYY-MM-DD>/`
