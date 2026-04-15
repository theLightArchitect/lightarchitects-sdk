# GUARD Report — Phase 14 Security Audit

**Build**: steady-forging-lynx
**Date**: 2026-03-22
**Scope**: lightarchitects-sdk workspace (lightarchitects-core, lightarchitects-{soul,corso,eva,quantum,seraph,ayin,crypto,cli,arena,auth}, lightarchitects umbrella)
**Auditor**: CORSO GUARD + manual review
**Verdict**: PASS — zero HIGH or CRITICAL findings

---

## 1. STRIDE Threat Model

### Attack Surface

The SDK is a local-process stdio JSON-RPC client. There is no network socket, no HTTP server,
and no external-facing API. The attack surface is:

1. **Binary path** — the SDK resolves sibling binaries from `$HOME/{sibling}/bin/{binary}`
2. **Child process stdio** — requests and responses transit through piped stdin/stdout
3. **Deserialised MCP responses** — JSON parsed from child stdout
4. **Cryptographic material** — key derivation (HKDF), signing (Ed25519), encryption (AES-256-GCM)
5. **Environment variables** — API keys and credentials inherited by child processes

---

### S — Spoofing

| ID | Threat | Vector | Severity | Mitigation |
|----|--------|--------|----------|------------|
| S1 | Binary impersonation | Attacker with filesystem write access replaces `~/lightarchitects/soul/bin/soul` with a malicious binary | MEDIUM | Requires home-dir write access (user-level compromise). No codesign verification in SDK. Mitigation: OS file permissions (`chmod 755`, owned by user). |
| S2 | Response ID injection | Malicious binary sends a response with a different `id` to correlate to a different request | LOW | ID correlation enforced in `read_response`: mismatched IDs return `ProtocolError::IdMismatch` and are rejected. |

### T — Tampering

| ID | Threat | Vector | Severity | Mitigation |
|----|--------|--------|----------|------------|
| T1 | Response tampering in transit | Attacker intercepts piped stdio to inject crafted JSON | LOW | Piped stdio is an OS-level IPC primitive; tampering requires the same process privileges as the parent. No practical vector for external attackers. |
| T2 | Binary replacement post-connect | Binary replaced after `StdioTransport::connect` establishes the child process | NONE | The child process handle is held by `StdioInner`. Replacing the binary on disk does not affect the already-running process. |

### R — Repudiation

| ID | Threat | Vector | Severity | Mitigation |
|----|--------|--------|----------|------------|
| R1 | No SDK-level request audit trail | Requests and responses are not logged by the SDK | INFO | Intentional: SDK logs at `tracing::debug!`. Production audit trails use the AYIN observability layer (`lightarchitects-ayin`), which wraps the transport and emits structured spans. |

### I — Information Disclosure

| ID | Threat | Vector | Severity | Mitigation |
|----|--------|--------|----------|------------|
| I1 | API key inheritance | `ANTHROPIC_API_KEY` and other credentials pass to child processes via env inheritance | INFO | **Intentional design**: sibling binaries need these keys to function. The transport does not call `env_clear()` because the children are trusted, owned binaries. This is documented in transport.rs. |
| I2 | Key material in memory | Derived key bytes (HKDF output) could persist in memory after use | LOW | `DerivedBytes` wraps `Zeroizing<[u8; 32]>`, zeroing on drop. IKM passed to HKDF also wrapped in `Zeroizing`. `SecretString` used for pass-phrases. |
| I3 | Debug logging of secrets | `tracing::debug!` or `Display` impls on secret-holding types | INFO | `DerivedBytes` does not implement `Display`. `SecretString` from the `secrecy` crate redacts in `Debug`. No accidental log exposure path found. |

### D — Denial of Service

| ID | Threat | Vector | Severity | Mitigation |
|----|--------|--------|----------|------------|
| D1 | Oversized response body | Malicious sibling sends multi-GB response body | LOW | `MAX_RESPONSE_BYTES` (10 MiB) enforced in both `read_newline_frame` and `read_content_length_frame` *before* extending the allocation buffer. |
| D2 | Infinite newline-frame read | Malicious sibling sends bytes forever without a newline | LOW | `read_newline_frame` checks `saturating_add(n) > MAX_RESPONSE_BYTES` on every fill loop. Additionally, the per-call 30s timeout fires. |
| D3 | Infinite header loop (Content-Length) | Malicious SERAPH binary sends >32 headers before blank line | LOW | **Fixed in Phase 14**: `MAX_CONTENT_LENGTH_HEADERS = 32` enforced with a `header_count` guard. Returns `ProtocolError::UnexpectedShape` on excess. |
| D4 | Retry storm | Transient transport errors trigger unbounded retries | LOW | `RetryConfig::max_attempts` (default 3) bounds retry count. `ToolError` is explicitly excluded from retry (tool logic is not transient). |
| D5 | Timeout bypass | Request hangs indefinitely | LOW | `tokio::time::timeout` wraps the entire send+receive sequence. Default 30s. Configurable via `SiblingClientBuilder::timeout`. |

### E — Elevation of Privilege

| ID | Threat | Vector | Severity | Mitigation |
|----|--------|--------|----------|------------|
| E1 | Process escalation | SDK-spawned child gains elevated privileges | NONE | `Command::new(binary_path)` inherits the parent's user and group. No `setuid`, no capability manipulation. Child runs as the same user. |

---

## 2. Supply Chain Audit

### `cargo audit` result

```
Scanning Cargo.lock for vulnerabilities (343 crate dependencies)
warning: 1 allowed warning found
```

**RUSTSEC-2025-0119** (`number_prefix 0.4.0` — unmaintained):
- Severity: **WARNING** (not a CVE)
- Reach: `lightarchitects-arena` → `indicatif` → `number_prefix`
- `lightarchitects-arena` is an ML training utility crate, not part of the core SDK
- No safe upgrade path exists upstream
- **Decision**: ACCEPTED. Added to `deny.toml` ignore list with documented rationale.

### `cargo deny check` result

```
advisories ok, bans ok, licenses ok, sources ok
```

**Licenses added to allow list**:
- `MPL-2.0`: `option-ext 0.2.0` via `dirs` (lightarchitects-auth only). File-level copyleft; does not affect SDK consumers.
- `CDLA-Permissive-2.0`: `webpki-roots 1.0.6` (TLS certificate roots). Permissive, data-only license.

**Internal crate licensing**: All lightarchitects-* and ayin crates use `LicenseRef-LA-Proprietary` with `[[licenses.clarify]]` entries. None are published to crates.io.

---

## 3. Secrets and Cryptography Review

| Area | Finding | Status |
|------|---------|--------|
| Key derivation (HKDF) | IKM wrapped in `Zeroizing::new(...)` before use | MITIGATED |
| Derived bytes | `DerivedBytes(Zeroizing<[u8; 32]>)` — zeroed on drop | MITIGATED |
| Encryption IKM (AES-256-GCM) | `let ikm = Zeroizing::new(generate_bytes(32))` | MITIGATED |
| Signing seed (Ed25519) | `let ikm = Zeroizing::new(generate_bytes(32))` | MITIGATED |
| Pass-phrases | `SecretString` from `secrecy` crate — redacts in Debug | MITIGATED |
| Hardcoded secrets | grep found zero hardcoded keys, tokens, or credentials | CLEAN |
| SSH keys | No SSH key handling in scope (SERAPH SDK is separate) | N/A |

---

## 4. Input Validation Review

All MCP response deserialization boundaries are guarded:

| Layer | Validation |
|-------|-----------|
| Wire framing | `read_newline_frame` / `read_content_length_frame` — size bounds enforced |
| JSON parsing | `serde_json::from_str` — errors mapped to `ProtocolError::MalformedJson` |
| Response ID | Correlation check — `IdMismatch` on mismatch |
| `isError` field | Checked by all QUANTUM/SERAPH/EVA `unwrap_text` helpers |
| Typed deserialization | `serde_json::from_value::<T>` — schema validated via `#[derive(Deserialize)]` |
| Content-Length header count | `MAX_CONTENT_LENGTH_HEADERS = 32` enforced in header parse loop |

No unvalidated trust boundary crossings found.

---

## 5. Dependency Justification

Every production dependency is justified:

| Crate | Justification |
|-------|--------------|
| `tokio` | Async runtime — required for async stdin/stdout |
| `serde` / `serde_json` | JSON-RPC serialization |
| `thiserror` | Error type derivation (library-style errors) |
| `tracing` | Structured diagnostics on tool dispatch |
| `aes-gcm`, `ed25519-dalek`, `hkdf`, `hmac`, `sha2` | lightarchitects-crypto only; RustCrypto ecosystem |
| `zeroize`, `secrecy` | Secret management — defence against memory scraping |
| `rand` | Cryptographic random generation |
| `clap` | CLI arg parsing (lightarchitects-cli only) |
| `proptest` | Property-based tests (lightarchitects-crypto dev) |

No "bloat" dependencies found. All deps are feature-gated where applicable.

---

## 6. Findings Summary

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| F1 | MEDIUM | Binary path not signature-verified | ACCEPTED (user-level access required) |
| F2 | LOW | Content-Length header loop unbounded | FIXED (MAX_CONTENT_LENGTH_HEADERS = 32) |
| F3 | INFO | API key env inheritance (intentional) | DOCUMENTED |
| F4 | LOW | number_prefix unmaintained (RUSTSEC-2025-0119) | ACCEPTED (lightarchitects-arena only, no CVE) |
| F5 | INFO | MPL-2.0 / CDLA-Permissive-2.0 licenses | MITIGATED (allow list updated) |

**Zero HIGH or CRITICAL findings.**

---

## 7. Phase 14 Acceptance Criteria

- [x] STRIDE threat model for SDK architecture
- [x] `cargo audit` clean — one warning (RUSTSEC-2025-0119, accepted)
- [x] `cargo deny check` clean — all checks pass
- [x] Secret handling review: `Zeroizing`, `SecretString`, no plaintext key material
- [x] Transport security: env inheritance documented; Content-Length loop bounded
- [x] Input validation: all deserialization boundaries guarded with typed errors
- [x] Supply chain: every dep justified, no bloat
- [x] GUARD report with findings + severity (this document)
