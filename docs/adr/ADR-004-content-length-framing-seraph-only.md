# ADR-004: Content-Length Framing for SERAPH Only

**Status**: Accepted
**Date**: 2026-03-20
**Build**: steady-forging-lynx

## Context

MCP responses can be framed in two ways:
1. **Newline-delimited JSON** — each response is a complete JSON object terminated by `\n`
2. **`Content-Length` header framing** — HTTP-style headers followed by a blank line, then the body

SOUL, CORSO, EVA, and QUANTUM all use newline framing. SERAPH uses `Content-Length` framing because it is architecturally different: SERAPH is a dual-binary system (Mac bridge + Khadas ARM64) and the `Content-Length` format is the MCP spec default.

## Decision

`McpFraming` is an enum with two variants: `Newline` and `ContentLength`. `SiblingId` determines the framing for each sibling. `StdioTransport::connect` reads the `framing()` method on `SiblingId` and dispatches to either `read_newline_frame` or `read_content_length_frame`.

Both framing functions are generic over `AsyncBufRead + Unpin` to support adversarial testing with `BufReader<&[u8]>`.

## Rationale

1. **Correctness**: SERAPH's production binary emits `Content-Length` headers. Using newline framing would break the connection immediately.
2. **Encapsulation**: Callers of `SeraphClient` don't need to know which framing is used — the `SiblingId::Seraph` configuration handles it.
3. **Testability**: Making both frame-reading functions generic enables in-process adversarial unit tests without spawning any child process.
4. **Extensibility**: If a future sibling adopts a third framing format, adding a `McpFraming::WebSocket` variant is a localised change.

## Consequences

- `read_content_length_frame` parses `Content-Length: N` headers and reads exactly N bytes.
- The header count is bounded by `MAX_CONTENT_LENGTH_HEADERS = 32` (D3 STRIDE mitigation).
- The body size is bounded by the same `MAX_RESPONSE_BYTES = 10 MiB` limit as newline framing (D1 STRIDE mitigation).
