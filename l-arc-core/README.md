# l-arc-core

Wire protocol, stdio transport, retry, and error types for the `l-arc` SDK.

Provides [`McpClient`], [`StdioTransport`], [`SiblingId`], and [`SdkError`] — the
foundation on which all sibling-specific clients (`l-arc-soul`, `l-arc-corso`, etc.)
are built.

## Key Types

| Type | Purpose |
|------|---------|
| `Transport` | Async trait over the MCP stdio wire |
| `StdioTransport` | Spawns a sibling binary; handles newline and `Content-Length` framing |
| `McpClient<T>` | Retry-aware generic client |
| `SiblingId` | Per-sibling binary path, framing, and orchestrator tool name |
| `SdkError` | Unified error hierarchy |
| `Config` / `RetryConfig` | Client and retry configuration |
