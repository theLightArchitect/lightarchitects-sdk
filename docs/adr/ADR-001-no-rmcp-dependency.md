# ADR-001: Do Not Depend on `rmcp`

**Status**: Accepted
**Date**: 2026-03-20
**Build**: steady-forging-lynx

## Context

`rmcp` is a community Rust crate for MCP (Model Context Protocol) clients. At the time of this decision, the in-production `mcp_pool.rs` pattern (used across all five sibling binaries) was battle-tested and stable. The `rmcp` API was still evolving.

## Decision

`lightarchitects-sdk` does not depend on `rmcp`. The MCP stdio transport is implemented directly in `lightarchitects-core` using `tokio::process::Command`, `BufReader<ChildStdout>`, and `ChildStdin`.

## Rationale

1. **API stability**: `rmcp` was undergoing frequent changes at evaluation time. An SDK is a contract; we cannot let an unstable upstream break our API.
2. **Trust the battle-tested path**: The `mcp_pool.rs` pattern in the sibling binaries works in production. Re-implementing it cleanly is lower risk than adopting a new crate.
3. **Minimal footprint**: `lightarchitects-core` has only 5 production dependencies. Adding `rmcp` would introduce its full dependency tree.
4. **Security**: Every dependency is a supply chain exposure. The STRIDE threat model for `lightarchitects-sdk` prefers minimal deps.

## Consequences

- We own the full transport implementation, including JSON-RPC serialization, framing (newline and `Content-Length`), and retry logic.
- When `rmcp` stabilizes, we can evaluate adoption — but the bar is "demonstrably better" not just "available".
- Fuzzing targets (`read_newline_frame`, `read_content_length_frame`) are well-defined because we own the code.
