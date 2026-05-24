# Capability-Coverage Matrix — `lightarchitects-gateway`

Maps every test file to production capabilities. If every test in a capability block passes, that capability is **production-ready**.

---

## Capabilities (13)

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
| M | **Skills-as-Tools** | Operator-wins invariant, turn-boundary lifecycle, SKILL.md content hash pinning, tamper detection via trust ledger |

---

## Test-to-Capability Map

| Test File | Capabilities Covered | Notes |
|---|---|---|
| `tests/e2e.rs` | A, J | MCP stdio handshake, tools/list, tools/call round-trip |
| `tests/ui_wire_test.rs` | D, J | HTTP wire protocol, gateway↔webshell dispatch via axum mock |
| `tests/live_headed_tests.rs` | H, J | CLI chat REPL, binary spawn, `--version`, `status`, `routes` |
| `tests/vault_cli_tests.rs` | C, I | Vault clone, validate-for-push, sync-public, wikilink scanning |
| `src/rubric.rs` (unit tests) | L | Score band boundaries, aggregate computation, component clamping, persistence roundtrip |
| `src/conversational.rs` (unit tests) | H | Brainstorm, plan extraction, empty session, budget, monotonicity |
| `tests/skills_as_tools_integration.rs` | M | Operator-wins turn boundary lifecycle; trust ledger pin+verify (Canon XXVII Suite 2) |
| `tests/property_skill_trust.rs` | M | Public `verify_or_pin` API: first-call-ok and determinism properties (Canon XXVII Suite 3) |
| `tests/regression_phase6.rs` | M | R6-1 writeln! ledger round-trip; R6-2 let-else no-panic; R6-3 LazyLock regex init (Canon XXVII Suite 5) |
| `tests/smoke.rs` (Phase 6 additions) | M | `GatewayToolExecutor` construction; `verify_or_pin` smoke with stable slug (Canon XXVII Suite 6) |
| `src/cli/skill_trust.rs` (unit + proptest) | M | `sha256_content` 64-hex-chars, determinism, avalanche; ledger round-trip; tamper detection (Canon XXVII Suite 1) |

---

## Gaps

| Capability | Missing Coverage | Priority |
|---|---|---|
| A | Agent dispatch end-to-end (spawn + stream + render via UI) | HIGH |
| B | LASDLC build lifecycle — no dedicated gateway integration tests | MEDIUM |
| D | ToolRegistry integration tests (25+ tools, plugin scanner) | HIGH |
| E | TUI snapshot tests — gateway has no ratatui/insta deps | MEDIUM |
| F | Session fork/merge end-to-end | LOW |
| G | Container pipeline — tested in `lightarchitects-webshell` crate only | MEDIUM |
| I | Security boundary integration tests (injection, path traversal, HMAC) | MEDIUM |
| K | State machine / task queue (scheduler, cascade failure) | MEDIUM |
| L | Calibration drift detection — `RubricStore` query exists but no integration test | LOW |
| M | `run_skill_tool()` subprocess dispatch — **CLOSED** (`tests/skills_gateway_e2e.rs`, 2026-05-23). Fix: `LIGHTARCHITECTS_BIN` env var override checked before `current_exe()`. 4 tests: binary smoke, unknown-skill error capture, trust gate tamper block, operator-wins gate. | CLOSED |

---

## Rubric: LASDLC C1-C8

Same definitions as `lightarchitects-webshell/tests/CAPABILITY_MATRIX.md`.

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
