# ADR-003: Retry Transport Errors Only — Never Tool Errors

**Status**: Accepted
**Date**: 2026-03-20
**Build**: steady-forging-lynx

## Context

`McpClient<T>` wraps any `Transport` with automatic retry logic. The question is: which errors should trigger a retry?

## Decision

`RetryConfig` retries only `TransportError::Timeout` and `TransportError::Io`. It explicitly does NOT retry:

- `ToolError` — the sibling returned an `isError: true` MCP response
- `ProtocolError` — malformed JSON, ID mismatch, unexpected response shape
- `SerializationError` — request could not be serialized
- `ConfigError` — client is misconfigured

## Rationale

**Transport errors are transient.** A timeout or I/O error means the child process was temporarily unavailable (startup race, temporary resource pressure). Retrying makes sense.

**Tool errors are deterministic.** If SERAPH returns `isError: true` because the engagement scope has expired, retrying the same call will get the same error. Retrying wastes tokens and time, and could mask a real configuration problem.

**Protocol errors signal a bug.** If the response is malformed JSON, retrying sends another request to the same buggy binary. The right action is to surface the error and let the caller investigate.

## Consequences

- `RetryConfig::should_retry` inspects the `SdkError` variant directly.
- Callers that want to retry tool errors must implement their own retry loop with appropriate backoff and scope management.
- The default retry count (3) with exponential backoff and 0.75 jitter handles the common transient case without overwhelming the child process.
