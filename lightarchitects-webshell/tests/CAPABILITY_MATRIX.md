# Webshell Capability-Coverage Matrix

> If every test mapped to a capability passes, that capability is production-ready.
> This matrix maps each `lightarchitects-webshell/tests/` file and `e2e/` spec to one
> of the 12 fundamental capabilities.

## Capabilities

| ID | Capability | Description | Prod-Ready Gate |
|---|---|---|---|
| C01 | Agent Dispatch | Spawn, fanout, cancel, scope enforcement, 8-sibling coverage | `cargo test --test phase_e_multi_build` |
| C02 | LASDLC Build Lifecycle | Plan, build, verify, deploy, secure, observe, enrich | `cargo test --test phase_c_wire` + `phase_d_stubs` |
| C03 | Vault / Helix | Query, inject, sanitize, pre-push encryption, soul-public sync | `cargo test --test helix_recursion` |
| C04 | Tool Ecosystem | 25+ tools via ToolRegistry, plugin scanner, permission gates | `cargo test --test fullstack_contract` |
| C05 | TUI / Webshell UI | ratatui rendering, 3D helix panel, build tracker, settings overlay | `pnpm exec vitest run` + `pnpm exec playwright test` |
| C06 | Session Management | Fork, merge, SQLite persistence, auth token isolation | `cargo test --test session_fork` |
| C07 | Container & Infra | Docker probe, image provisioning, spawner, init profiler, telemetry, shutdown | `cargo test --test container_e2e` + `init_e2e` |
| C08 | Conversational Mode | Interactive chat, brainstorming, context accumulation, no-build iteration | `pnpm exec playwright test e2e/conversational.spec.ts` |
| C09 | Security Boundaries | Injection sanitization, logging redaction, path traversal, HMAC, confusables | `cargo test --test adversarial_e2e` + `authorization` |
| C10 | MCP Mesh | Sibling spawn, reconnect, health, custom binary validation | `cargo test --test mcp_sdk` (SDK side) |
| C11 | State Machine / Task Queue | Dependency DAG, cycle detection, cascade failure, scheduler | `cargo test --test chaos` + `idempotency` |
| C12 | Output Grading | Rubric application (C1-C8), score persistence, calibration drift | `cargo test --test rubric_engine` + Playwright `rubric-score.spec.ts` |

## File-to-Capability Map — Rust Backend Tests

| Test File | Primary Capability | Secondary | Status |
|---|---|---|---|
| `adversarial_e2e.rs` | C09 Security | C02 LASDLC | ✅ |
| `authorization.rs` | C09 Security | — | ✅ |
| `chaos.rs` | C11 State Machine | — | ✅ |
| `contract.rs` | C04 Tools | C02 LASDLC | ✅ |
| `fullstack_contract.rs` | C02 LASDLC | C04 Tools | ✅ |
| `helix_recursion.rs` | C03 Vault | — | ✅ |
| `idempotency.rs` | C11 State Machine | — | ✅ |
| `phase_c_wire.rs` | C02 LASDLC | — | ✅ |
| `phase_d_stubs.rs` | C02 LASDLC | — | ✅ |
| `phase_e_auth_profile.rs` | C09 Security | C06 Session | ✅ |
| `phase_e_multi_build.rs` | C02 LASDLC | C01 Agent Dispatch | ✅ |
| `user_journey.rs` | C02 LASDLC | C01 Agent Dispatch | ✅ |

## Gaps — Rust Backend

| Missing Test | Capability | What It Covers |
|---|---|---|
| `container_e2e.rs` | C07 Container & Infra | Docker probe, ImageManager idempotency, spawner rejection, embedded Dockerfile build |
| `init_e2e.rs` | C07 Container & Infra | Profiler checkpoint events, telemetry UUID hashing, shutdown registry grace |
| `session_persistence.rs` | C06 Session | SQLite roundtrip, concurrent writes, touch timestamp, noop store |

## File-to-Capability Map — UI (Playwright E2E)

| Spec | Capability | Status |
|---|---|---|
| `e2e/webshell.spec.ts` | C05 TUI / Webshell UI | ✅ |
| `e2e/agent-dispatch.spec.ts` | C01 Agent Dispatch | 🔴 MISSING |
| `e2e/build-lifecycle.spec.ts` | C02 LASDLC | 🔴 MISSING |
| `e2e/vault-query.spec.ts` | C03 Vault | 🔴 MISSING |
| `e2e/session-fork.spec.ts` | C06 Session | 🔴 MISSING |
| `e2e/container-toggle.spec.ts` | C07 Container | 🔴 MISSING |
| `e2e/conversational.spec.ts` | C08 Conversational | 🔴 MISSING |
| `e2e/rubric-score.spec.ts` | C12 Output Grading | 🔴 MISSING |

## Running the Matrix

```bash
# Rust backend — all integration tests
cd lightarchitects-webshell
cargo test --test '*'

# UI unit tests
cd lightarchitects-webshell-ui
pnpm exec vitest run

# UI E2E (headed only — headless: false per policy)
PLAYWRIGHT_BASE_URL=http://localhost:5173 pnpm exec playwright test
```
