# Capability-Coverage Matrix — `lightarchitects-webshell`

Maps every test file to production capabilities. If every test in a capability block passes, that capability is **production-ready**.

---

## Capabilities (12)

| ID | Capability | Definition |
|---|---|---|
| A | **Agent Dispatch** | Spawn, fanout, cancel, scope enforcement, sibling coverage |
| B | **LASDLC Build Lifecycle** | Plan, build, verify, deploy, secure, observe, enrich |
| C | **Vault / Helix** | Query, inject, sanitize, pre-push encryption, soul-public sync |
| D | **Tool Ecosystem** | 25+ tools via ToolRegistry, plugin scanner, permission gates |
| E | **TUI / Webshell UI** | ratatui rendering, 3D helix panel, build tracker, settings overlay |
| F | **Session Management** | Fork, merge, SQLite persistence, auth token isolation |
| G | **Container & Infra** | Docker probe, image provisioning, spawner, init profiler, telemetry, shutdown |
| H | **Conversational Mode** | Interactive chat, brainstorming, context accumulation, no-build iteration |
| I | **Security Boundaries** | Injection sanitization, logging redaction, path traversal, HMAC, confusables |
| J | **MCP Mesh** | Sibling spawn, reconnect, health, custom binary validation |
| K | **State Machine / Task Queue** | Dependency DAG, cycle detection, cascade failure, scheduler |
| L | **Output Grading** | Rubric application, score persistence, calibration drift detection |

---

## Test-to-Capability Map

| Test File | Capabilities Covered | Notes |
|---|---|---|
| `tests/container_e2e.rs` | G | Docker probe, ImageManager, spawner, embedded image strings |
| `tests/init_e2e.rs` | G | Profiler checkpoints, telemetry hashing, shutdown registry LIFO + panic isolation |
| `tests/session_persistence.rs` | F | SQLite roundtrip, concurrent writes, touch semantics, noop store |
| `tests/adversarial_e2e.rs` | I | Injection sanitization, path traversal, HMAC, confusables |
| `tests/authorization.rs` | I | Auth profile, multi-build authorization gates |
| `tests/chaos.rs` | K | Cascade failure, scheduler under stress |
| `tests/contract.rs` | D, J | Type contracts, binary framing, tool registry validation |
| `tests/e2e/webshell.spec.ts` | E | Playwright E2E — headed, UI smoke test |
| `tests/fullstack_contract.rs` | B, K | LASDLC build lifecycle + task queue end-to-end |
| `tests/idempotency.rs` | K | Idempotency keys, duplicate request rejection |
| `tests/phase_c_wire.rs` | J | MCP mesh wire protocol, reconnect, heartbeat |
| `tests/phase_d_stubs.rs` | D | Tool stub registry, permission gates |
| `tests/phase_e_auth_profile.rs` | I | Auth profile validation, token isolation |
| `tests/phase_e_multi_build.rs` | B, K | Multi-build orchestration, dependency DAG |
| `tests/user_journey.rs` | E, H | User journey E2E — onboarding, chat, build promotion |
| `tests/vault_cli_tests.rs` | C, I | Vault clone, validate-for-push, sync-public |

---

## Gaps

| Capability | Missing Coverage | Priority |
|---|---|---|
| A | Agent dispatch end-to-end (spawn + stream + render) | HIGH |
| E | TUI snapshot tests (insta + ratatui) | MEDIUM |
| H | Conversational mode backend tests (webshell chat route) | MEDIUM |
| L | Rubric UI component (Svelte radar chart) | LOW |

---

## Rubric: LASDLC C1-C8

Same definitions as `lightarchitects-gateway/tests/CAPABILITY_MATRIX.md`.

| Component | Weight | Scoring Method |
|---|---|---|
| C1 — Output Completeness | 10% | Checklist matching |
| C2 — Validation Discipline | 15% | Cross-validation flag + citations |
| C3 — Gate Compliance | 15% | Gate runner verdict aggregation |
| C4 — Operator Experience | 10% | Jargon penalty + conciseness reward |
| C5 — Resource + Trace Discipline | 10% | Token count buckets + span coverage |
| C6 — Iteration Integrity | 10% | Loop-cycle / oscillation detection |
| C7 — Northstar Alignment | 15% | Keyword overlap task → output |
| C8 — Context Precision | 15% | Extra-topic penalty (hallucination guard) |

Score bands: Exemplary (90-100) · Strong (75-89) · Acceptable (60-74) · Deficient (45-59) · Unsafe (<45)
