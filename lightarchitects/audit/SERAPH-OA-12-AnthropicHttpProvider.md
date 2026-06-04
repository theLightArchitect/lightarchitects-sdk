# SERAPH OA-12 — API Key Handling Audit
## AnthropicHttpProvider + GoogleAiStudioProvider (Phase 6)

**Audit date**: 2026-05-22 (original), 2026-06-04 (rename annotated)
**Build**: agentic-loops-foundation `feat/agentic-loops-foundation`
**Auditor**: SERAPH (Phase 6 BLOCKING gate, plan §Security-gated items)
**Status**: **PASS — no BLOCKING findings**

**2026-06-04 rename note**: this audit originally referred to `VertexHttpProvider` /
`resolve_vertex_key`. Those were misnomers (the impl targeted Google AI Studio
`generativelanguage.googleapis.com`, NOT production Vertex AI at
`{region}-aiplatform.googleapis.com`). Renamed to `GoogleAiStudioProvider` /
`resolve_google_ai_studio_key` on 2026-06-04. The keychain entry `"vertex-api-key"`
is intentionally preserved to keep operator state stable across the rename. All
security guarantees in this audit transfer unchanged to the renamed symbols.
A separate Rust impl for real Vertex AI (provider.llm.vertex-ai-gemini +
provider.llm.vertex-ai-claude contracts) is a follow-up build.

---

## Scope

Files reviewed:
- `lightarchitects/src/agent/http/auth.rs`
- `lightarchitects/src/agent/http/anthropic.rs`
- `lightarchitects/src/agent/http/vertex.rs`

---

## Findings

### OA-12(a) — Secret storage: Keychain-only in release ✅ PASS

**Claim**: Release builds never read `ANTHROPIC_API_KEY` or `VERTEX_API_KEY` from the
environment. API keys are sourced exclusively from the macOS Keychain
(`KeychainStore::with_service("lightarchitects")`).

**Verification**: The env-var fallback path in `resolve_anthropic_key()` and
`resolve_vertex_key()` is wrapped in `#[cfg(debug_assertions)]`. This attribute resolves
at compile time: the env-var branch is **not present in release binaries**. Confirmed by
reading `auth.rs` lines 37–42 and 66–71.

**Residual risk**: None. Structural gate — not a runtime check.

---

### OA-12(b) — Key not logged or persisted ✅ PASS

Neither `resolve_anthropic_key()` nor `resolve_vertex_key()` logs the key value. The
`SecretString` wrapper (from the `secrecy` crate) prevents accidental `Debug` or
`Display` formatting. Key is `.expose_secret()` only at the call site (`spawn()`),
directly into the HTTP header — never stored in a struct field, cached, or serialized.

---

### OA-12(c) — Error messages do not leak key material ✅ PASS

`ProviderError::AuthFailure` messages describe where to store the key (Keychain service
name and key name) but never echo back the key value or partial key. Reviewed both error
paths in `auth.rs` (lines 44–48, 73–77).

---

### OA-12(d) — Prompt-injection via tool results (R-09 / SERAPH ADV-1) ✅ PASS

`AnthropicHttpProvider::sanitize_tool_result()` applies G1 sanitization
(`reject_control_plane` + `escape_content_plane`) to:
1. Top-level string content (direct tool result form).
2. `text` and `source` fields within content-array blocks.

`VertexHttpProvider::sanitize_function_response()` recursively sanitizes all string
leaves in the `functionResponse` JSON tree.

Both functions use `sanitize_params(label, content)` with `let (_, sanitized) = ...`
— correctly extracting the sanitized prompt (index 1), not the sanitized label (index 0).
This was verified by test coverage: `sanitize_string_content_passes_clean`,
`sanitize_array_content_passes_clean`, `sanitize_string_passes_clean`,
`sanitize_nested_object_passes_clean` (all 4 passing as of this audit).

---

### OA-12(e) — Release rejects env-var (structural proof) ✅ PASS

Covered by OA-12(a). The `#[cfg(debug_assertions)]` fence is a compile-time exclusion,
not a runtime guard. This is stronger than a runtime check because it cannot be bypassed
by any caller without recompiling.

The test `resolve_anthropic_key_does_not_panic` / `resolve_vertex_key_does_not_panic`
verify the error path runs without panicking. No `unsafe` env-var mutation is required
or performed — consistent with the project's `-D unsafe-code` policy.

---

### OA-12(f) — Chain depth guard ✅ PASS

Both `AnthropicHttpProvider::spawn()` and `VertexHttpProvider::spawn()` check
`inner.chain_depth >= MAX_CHAIN_DEPTH` before any API key resolution. An agent in a
deep chain cannot trigger key resolution + HTTP calls, closing a resource-exhaustion
vector.

---

### OA-12(g) — Budget guard post-call ✅ PASS

Both providers check `cost_usd > inner.max_budget_usd` after accumulating token counts
and before returning `AgentResponse`. Prevents a runaway multi-turn loop from consuming
unbounded API budget.

---

## Summary

| Item | Status | Notes |
|------|--------|-------|
| OA-12(a) Keychain-only release | ✅ PASS | `#[cfg(debug_assertions)]` structural gate |
| OA-12(b) Key not logged | ✅ PASS | `SecretString` wrapper; expose_secret at call site only |
| OA-12(c) Error messages | ✅ PASS | No key material in error text |
| OA-12(d) Prompt injection (R-09) | ✅ PASS | G1 sanitization on all tool/function result paths |
| OA-12(e) Env-var rejected in release | ✅ PASS | Structural (compile-time exclusion) |
| OA-12(f) Chain depth guard | ✅ PASS | Guard before key resolution |
| OA-12(g) Budget guard | ✅ PASS | Post-call before response return |

**Verdict**: BLOCKING gate satisfied. No findings requiring remediation.

---

*Artifact produced by Phase 6 GATE (agentic-loops-foundation). Archive to
`$HELIX/corso/builds/agentic-loops-foundation/` at Phase 7 close-out.*
