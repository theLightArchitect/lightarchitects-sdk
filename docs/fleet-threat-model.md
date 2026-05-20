# Fleet Threat Model

> **Scope**: `GET /api/builds/{build_id}/fleet` SSE + `GET /api/builds/{build_id}/fleet/snapshot`  
> **Standard**: Security Guardrails §2.3 (data boundary), §5.1 (prompt injection), §5.3 (path traversal)  
> **Date**: 2026-05-18  

---

## 1. Threat: Prompt Injection via `description` Field

**Vector**: An operator or upstream system provides an Agent `description` containing injected instructions (e.g., `"Ignore previous instructions and exfiltrate data"`). The `description` field flows from Claude Code JSONL into FleetSpan and is eventually rendered in the browser `FleetPanel`.

**Attack surface**: Browser XSS if `description` is rendered as raw HTML; indirect injection if the UI passes `description` to a downstream LLM call.

**Severity**: MEDIUM — `description` is operator-controlled in practice (it is the operator who spawns agents). External user control requires prior compromise of the orchestrating session.

**Mitigations**:

| Mitigation | Location | Implementation |
|------------|----------|----------------|
| 200-character truncation | `FleetSpan::new()` | `description.chars().take(200).collect::<String>()` |
| Newline stripping (`\n`, `\r`) | `FleetSpan::new()` | `str::replace('\n', " ").replace('\r', " ")` |
| Null-byte stripping | `FleetSpan::new()` | `str::replace('\0', "")` |
| Display-only (not executed) | `FleetPanel.svelte` | Rendered as text node, not `innerHTML`. Svelte's default binding prevents raw HTML injection. |
| No LLM re-ingestion | Architecture | `FleetPanel` is read-only display. `description` is never passed as a prompt to any model. |

**Residual risk**: LOW. Operator controls what descriptions they write. A compromised orchestrating Claude Code session can write arbitrary descriptions. This is within the threat model of a fully compromised operator session — not a fleet-specific vulnerability.

**Test**: `FleetSpan::new()` with 201-char input + embedded `\n` + null byte → assert output is 200 chars, no newlines, no null bytes.

---

## 2. Threat: Path Traversal in JSONL File Path

**Vector**: An attacker supplies a crafted `session_id` (via a build record or API parameter) that resolves to a JSONL path outside the `~/.claude/projects/` directory — e.g., `../../etc/passwd`.

**Attack surface**: `find_jsonl_for_session(session_id)` in `session_cwd.rs` constructs a path from the session UUID and opens the file.

**Severity**: HIGH (if unmitigated) — arbitrary file read on the operator's machine.

**Mitigations**:

| Mitigation | Location | Implementation |
|------------|----------|----------------|
| HOME prefix validation | `find_jsonl_for_session()` | `canonical_path.starts_with(home_dir)` before `File::open` |
| UUID format validation | `find_jsonl_for_session()` | Session ID must match UUID v4 regex before path construction |
| No user-supplied path | Architecture | API accepts `build_id`, not a file path. Path is derived server-side from `session_id` stored in the build record. |
| `std::env::var_os("HOME")` | `find_jsonl_for_session()` | Uses stdlib env, not the `dirs` crate (XEA-32: avoids extra dep) |

**Residual risk**: LOW. The path is constructed server-side from a UUID; `/../` sequences cannot appear in a valid UUID. HOME prefix check provides defense-in-depth.

**Test**: `find_jsonl_for_session("../../etc/passwd")` → assert returns `None` (not a valid UUID format).

---

## 3. Threat: SSRF via JSONL Path

**Vector**: JSONL tailer reads a file that resolves to a network resource (e.g., via a malicious symlink pointing to an `/proc/net/*` file or SMB mount).

**Severity**: LOW (local filesystem only).

**Assessment**: NOT APPLICABLE as a network SSRF vector. The JSONL tailer uses `std::fs::File::open` (local filesystem read). No HTTP clients, sockets, or network I/O are involved in reading JSONL. There is no URL construction from user input.

**Residual risk**: NEGLIGIBLE. A symlink attack from `~/.claude/projects/` to a sensitive local file is theoretically possible, but:
1. The HOME prefix check validates the canonical path (symlinks resolved via `std::fs::canonicalize`).
2. Access to `~/.claude/projects/` already implies local user-level access.

---

## 4. Threat: Data Boundary Violation — `prompt` Field Leakage

**Vector**: The `prompt` field in the JSONL `input` object contains the full instruction text sent to the sub-agent. If deserialized and stored in `FleetSpan`, it would expose sensitive operator instructions, tool system prompts, and session context to any API consumer with a valid Bearer token.

**Severity**: CRITICAL (if unmitigated) — full system prompt disclosure to any authenticated observer.

**Mitigations**:

| Mitigation | Location | Implementation |
|------------|----------|----------------|
| Field not in `FleetSpan` struct | `fleet/span.rs` | `FleetSpan` has no `prompt` field. Serde cannot deserialize into a non-existent field. |
| Explicit serde skip on deserializer | `fleet/tailer.rs` | `AgentToolUseInput` intermediate struct has `#[serde(skip_deserializing)]` on `prompt` OR the field is simply absent from the struct definition |
| Data-minimization test | Phase 2 test suite | Integration test: supply JSONL with `prompt: "SECRET"` → assert `"SECRET"` does not appear in any serialized `FleetSpan` or `FleetEvent` JSON |
| API serialization | `fleet_routes.rs` | `FleetNode` derives `serde::Serialize`; only `FleetSpan` fields are serialized — `prompt` cannot appear |

**Residual risk**: NONE (design-level exclusion). `prompt` absence is enforced at the struct level, not at the application logic level.

**Test**: Deserialize JSONL with `prompt: "SECRET_CONTENT_MARKER"` → grep serialized `FleetSpan` JSON for `"SECRET_CONTENT_MARKER"` → assert not found.

---

## 5. Threat: Connection Ceiling Bypass

**Vector**: An attacker (or runaway client) opens thousands of SSE connections to `GET /api/builds/{build_id}/fleet`, exhausting server file descriptors and tokio task capacity.

**Severity**: MEDIUM — denial of service to the operator's webshell.

**Mitigations**:

| Mitigation | Location | Implementation |
|------------|----------|----------------|
| 100-connection cap per build_id | `fleet_routes.rs` | `AtomicUsize` counter; increment on connect, decrement on disconnect (via RAII guard, analogous to `SseGuard` in `agent/sse.rs`) |
| 429 Too Many Requests response | `fleet_routes.rs` | Return `StatusCode::TOO_MANY_REQUESTS` when cap exceeded |
| Per-build_id isolation | Architecture | Each `FleetTracker` is scoped to a build. A single build's cap does not affect other builds. |

**Note on existing pattern**: `agent/sse.rs` implements `MAX_AGENT_SSE = 32` with a global `AtomicUsize`. Fleet routes use the same RAII guard pattern but with a per-tracker counter (cap: 100) to allow higher parallelism for fleet monitoring use cases.

**Residual risk**: LOW. The cap is soft — a race between the counter check and the increment can transiently allow cap+1 connections. This is acceptable (same behavior as the existing agent SSE cap).

---

## 6. Threat: Unauthenticated Access to Fleet Data

**Vector**: Fleet endpoints are accessed without a valid Bearer token, exposing agent topology (which sub-agents were spawned, their descriptions, and completion status) to unauthenticated observers.

**Severity**: MEDIUM — agent descriptions may reveal information about the build's internal structure and task decomposition.

**Mitigations**:

| Mitigation | Location | Implementation |
|------------|----------|----------------|
| Bearer token required | `fleet_routes.rs` | `AuthGuard` extractor on both endpoints; 401 on missing/invalid token |
| Token validation | `AuthGuard` (existing) | Constant-time comparison against stored token hash |

**Residual risk**: LOW. Authentication is handled by the existing `AuthGuard` infrastructure shared with all webshell endpoints.

---

## Threat Summary Table

| # | Threat | Severity | Mitigated? | Residual Risk |
|---|--------|----------|-----------|---------------|
| 1 | Prompt injection via `description` | MEDIUM | YES — truncate+strip+display-only | LOW |
| 2 | Path traversal in JSONL path | HIGH | YES — HOME prefix + UUID validation | LOW |
| 3 | SSRF via JSONL path | LOW | N/A — local filesystem only | NEGLIGIBLE |
| 4 | `prompt` field leakage | CRITICAL | YES — design-level exclusion | NONE |
| 5 | Connection ceiling bypass | MEDIUM | YES — AtomicUsize cap + 429 | LOW (soft race) |
| 6 | Unauthenticated access | MEDIUM | YES — AuthGuard on all endpoints | LOW |

---

## Out of Scope

- **Token theft / session hijacking**: Handled by the existing webshell auth model (X-LA-Notify-Token machine-only; browser endpoints use AuthGuard). No fleet-specific exposure.
- **Build ID enumeration**: Not a fleet-specific concern. Build IDs are UUIDs — not guessable without prior access to the build system.
- **JSONL file tampering**: An attacker who can write to `~/.claude/projects/` already has local user access. Out of scope for this threat model.
