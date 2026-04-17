# SERAPH Attack Pass — luminous-weaving-nautilus Phase 9

**Date**: 2026-04-13
**Scope**: `lightarchitects-webshell` v0.1.0 — local-dev-only, binds 127.0.0.1
**Attack surface**: HMAC auth, SSE endpoint, PTY WebSocket, static asset serving, token redaction
**Verdict**: GREEN — zero RED findings. All identified issues addressed before ship.

---

## Attack Vectors Tested

### 1. WS Token Smuggling — PASS (no RED)

**Vector**: Attacker attempts to extract the HMAC token from the WebSocket upgrade flow.

**Analysis**:
- Token travels as `Sec-WebSocket-Protocol: bearer.<token>` — not a URL query parameter, not a cookie, not a header that appears in typical access logs.
- `validate_ws_subprotocol` strips the `bearer.` prefix and compares the remainder with `constant_time_eq` — no timing side-channel.
- Session storage used by frontend (not `localStorage`) — cleared on tab close.

**Residual risk**: NEGLIGIBLE — token is in transit only within the local loopback. No external exposure path.

---

### 2. PTY Escape Injection — PASS (no RED)

**Vector**: Attacker injects terminal escape sequences through the agent command output to manipulate the terminal or exfiltrate data.

**Analysis**:
- The PTY bridge forwards raw bytes in both directions — xterm.js handles escape sequences on the client, which is the correct architectural boundary.
- PTY escape injection that affects the *server process* is not possible — the server reads from the PTY child's stdout; it does not interpret escape sequences server-side.
- `client_message_malformed_json_is_error` test confirms that malformed JSON control frames are rejected.

**Residual risk**: LOW — per local-dev threat model. Client terminal manipulation by the hosted agent is accepted behavior (the agent is trusted).

---

### 3. XSS via Step Metadata in SSE Payloads — PASS (no RED)

**Vector**: Attacker injects script tags or event handlers into AYIN span metadata fields that flow through the SSE stream into the React 3D scene.

**Analysis**:
- `WebEvent` is serialized by `serde_json::to_string` — all string values are JSON-escaped.
- React renders span data as React elements via JSX, not as raw HTML. The `actor` and `action` fields drive Three.js geometry labels and Zustand store state — no raw HTML injection path exists.
- Token redaction runs on the raw JSON before SSE emission: `redact()` replaces the HMAC token string before sending.
- `xss_payload_in_action_field_round_trips_cleanly` test: `<script>alert('xss')</script>` round-trips as an exact string, confirming JSON serialization preserves but does not execute the payload.

**Residual risk**: NEGLIGIBLE — no raw HTML rendering path in the frontend.

---

### 4. Sub-Protocol HMAC Forgery — PASS (no RED)

**Vector**: Attacker forges a `Sec-WebSocket-Protocol: bearer.<guessedtoken>` header to bypass auth.

**Analysis**:
- Token is a random 32-byte hex string (64 hex chars, 256 bits entropy). Brute-force is computationally infeasible.
- `constant_time_eq` prevents timing oracle — attacker cannot learn partial matches by measuring response time.
- `validate_ws_subprotocol` returns `false` on empty candidate, preventing zero-length token bypass.
- The token is not logged on failure (confirmed by `events_401_body_does_not_contain_token` test).

**Residual risk**: NEGLIGIBLE — 256-bit entropy + constant-time comparison.

---

### 5. Two-Tab Session Hijack — PASS (no RED)

**Vector**: Second browser tab or malicious page reuses an existing authenticated SSE connection.

**Analysis**:
- Each SSE `GET /api/events` opens an independent broadcast receiver. No shared session state.
- The PTY WebSocket enforces a concurrency cap of 4 (`MAX_SESSIONS`). The `claim_fails_at_cap` test confirms the 5th connection is rejected with 503.
- CORS allowlist (`http://localhost:8733`, `http://127.0.0.1:8733`, `http://localhost:5173`) prevents cross-origin tabs from reaching endpoints without a valid token.

**Residual risk**: NEGLIGIBLE — no shared session state; each connection independently authenticated.

---

## Findings Summary

| ID | Severity | Vector | Status | Fix Applied |
|----|----------|--------|--------|-------------|
| S-01 | MEDIUM | Wildcard CORS (`CorsLayer::permissive`) | FIXED | Explicit localhost allowlist + `cors_header_absent_for_unknown_origin` test |
| S-02 | MEDIUM | Bearer scheme not fully case-insensitive (rejected `BEARER`) | FIXED | `eq_ignore_ascii_case` + `bearer_accepts_uppercase_scheme` test |
| S-03 | LOW | No resize-frame rate-limit (malformed JSON CPU burn) | DEFERRED | Accepted for local-dev; rate-limit tracked as v1.1 hardening |
| S-04 | LOW | Token redaction not constant-time | INFO | Accepted: redaction is post-auth; no exploitation path |
| S-05 | INFO | `Response::builder().unwrap_or_else` reachability | INFO | Accepted: mime from rust-embed, not user input |
| S-06 | INFO | SIGTERM wait always 2s | INFO | Latency issue, not security; tracked as v1.1 improvement |

**RED findings**: 0
**SERAPH verdict**: GREEN — ship authorized from security perspective.

---

## Gate Status

| Gate | Result |
|------|--------|
| `gate_4_security` — HMAC audit | PASS |
| `gate_4_security` — PTY escape injection test | PASS |
| `gate_4_security` — redaction regex sweep | PASS |
| Phase 9 exit: SERAPH no RED | PASS |

---

*Generated during Phase 9 hardening pass. Reviewed against manifest risk register (RL_R7 = AMBER → MITIGATED).*
