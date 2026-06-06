# Builders Cookbook — Amendment History

Companion changelog for `builders-cookbook.md`. The cookbook holds **current state only**; this file holds the **amendment narrative** — section added, source build, canon reference, rationale — that `git log` doesn't capture in narrative form.

**Authoritative latest version**: see the header inline summary in `builders-cookbook.md`.
**Mechanical history**: `git log -- standards/canon/builders-cookbook.md`
**Constitutional basis**: Canon XLII — Schema-Changelog Separation (RATIFIED 2026-05-18 Phase 7). See `canon://platform-canon` §"Canon XLII".

---

## v3.14.0 — §63.P5 Tolerant deserialization for SCR1-F1 untrusted-producer JSONL/IPC (2026-06-05, Path B Phase 1 /REFLECT → LÆX ratification — stamped)

**Source**: /REFLECT session 2026-06-05 after Path A (D2 cockpit drawer keystone contract) + Path B Phase 1 (B1 JSONL wave_context propagation + B2 gh CLI PR fetch). One promotion candidate ratified, one rejected.

### §63.P5 — Tolerant deserialization for SCR1-F1 untrusted-producer JSONL/IPC (NEW pattern)

**Background**: During B1 implementation (`lightarchitects/src/fleet/jsonl.rs` `wave_context` field on `AgentToolInput`), the initial bare `Option<WaveContextInput>` field caused the entire `AgentToolInput` deserialization to fail when wave_context was malformed (wrong JSON type). Result: critical sibling fields (description, subagent_type) were lost, `agent_spawned` never fired, the agent silently disappeared from fleet tracking.

The `wave_context_malformed_does_not_block_spawn` test surfaced this. Fix landed in the same PR via a `deserialize_with` helper: `deserialize_wave_context_or_none` deserializes to `serde_json::Value` first, attempts typed conversion, falls to `None` on any error.

**Rule** (§63 uniform schema):
- **Threat**: CWE-20 Improper Input Validation — silent loss of an entire parse record when an upstream producer emits one malformed sub-field
- **Vector**: New `Option<T>` field on JSONL/IPC parse struct from untrusted producer (`/BUILD` wave-dispatcher, Claude Code session JSONL, sibling subprocess stdout, webhook payloads)
- **Mitigation**: `#[serde(default, deserialize_with = "tolerant_or_none")]` with the Value-first helper pattern
- **Static-lint rule**: forbid bare `Option<T>` on new Deserialize fields where SCR1-F1 forward-compat tolerance is the stated goal
- **Test vector**: malformed-field test asserting whole-record preservation

**Boundary clarification (LÆX condition)**: This pattern applies ONLY where `deny_unknown_fields` is intentionally absent for SCR1-F1 forward-compat. For strict HTTP request bodies, Security-Guardrails §3 `deny_unknown_fields` remains canonical — §63.P5 does NOT extend there.

**Evidence**: PROVISIONALLY_VALID (N=1). Promote to VALIDATED on second independent instance in another untrusted-producer parse path.

**LÆX verdict**: CONDITIONALLY_RATIFIED — conditions applied: (1) §63 uniform schema reframing, (2) PROVISIONALLY_VALID N=1 framing, (3) scope-clarification against Security-Guardrails §3.

**Operator stamp**: Kevin, 2026-06-05 — RATIFIED with LÆX conditions.

### Rejected from same /REFLECT batch — for traceability

**Axum `Path` vs `std::path::Path` collision in route modules** — REJECTED from §63 promotion by LÆX. §63 schema requires CWE anchor; the collision pattern is ergonomics/readability with no threat surface. No existing Rust handler-conventions canon section to house it. Re-routed to memory entry `feedback_axum_path_filesystem_collision.md`. DEFERRED for future canon promotion if a Rust handler-conventions section is created with ≥3 conventions justifying the substrate.

**Source documents**:
- Promotion candidates: `~/Projects/lightarchitects-sdk/arch/cookbook-63-promotion-candidates-2026-06-05.md` (now status: RATIFIED for C1 / REJECTED→memory for C2)
- LÆX session log: `helix/laex0/journal/invocations/2026-06-05/{HH-MM}-canon_ratification.md`



**Source**: /REFLECT session post `litellm-platform-integration` plan-authoring (6 iterations). Two canon sections landed and stamped in this batch:

### §50.10 — Two-Envvar Opt-In for Real-Infrastructure Tests (NEW section landing — closes 2026-05-30 promotion-gap)

**Background**: Memory `feedback_e2e_two_envvar_opt_in.md` claimed PROMOTED to §50.10 on 2026-05-30 but `grep -n "§50.10"` in the cookbook returned ZERO hits. LÆX surfaced this gap during 2026-05-31 Canon XXXIX Step (c) sweep. This v3.13.0 closes the gap by landing the verbatim rule.

**Rule**: Tests calling real APIs / databases / paid infra with wall-clock >5s or dollar cost >0 MUST be gated on TWO env vars (opt-in flag + credential). Reference implementation in §50.10 body. Cross-referenced by Blueprint §8.9.1 (sister ratification 2026-05-31) and Cookbook §69.1 (sister ratification 2026-05-31).

**Operator stamp**: Kevin, 2026-05-31 — RATIFIED.

### §69.1 — Integration Claim Verification (operator stamp landed)

**Rule** (from v3.12.0): Plan claims of integration ("X federates from Y") MUST be verified via grep against integration code. Zero hits → integration is fiction.

**Operator stamp**: Kevin, 2026-05-31 — RATIFIED (previously "pending"). v3.12.0 superseded by v3.13.0.

**Composition note**: §50.10 + §69.1 form a verify-twice pattern. §69.1 verifies the integration exists at plan-time via grep. §50.10 verifies it works at test-time via gated real-infra rehearsal. Together they cover both citation-discipline (claim-time) and execution-discipline (test-time) for the integration class.

---

## v3.12.0 — §69.1 Integration Claim Verification (2026-05-31, litellm-platform-integration /REFLECT — LÆX Candidate B)

**Source**: /REFLECT session post `litellm-platform-integration` plan-authoring (6 iterations). Canon XXXIX Step (c) contradiction check + Step (d) ratification verdict by LÆX. **Superseded by v3.13.0** (operator stamp landed + §50.10 sister section landed in same batch).

### §69.1 — Integration Claim Verification (new sub-section under §69 Citation Integrity Doctrine)

**Rule**: Any plan claim of the form "X integrates with Y" / "X federates from Y" / "X consumes Z's output" MUST be verified by grep against the integration code itself BEFORE plan reaches VALIDATED status. Integration code IS the citation; grep is the verification. Zero hits → integration is fiction; demote claim to deferred OR build the integration this build.

**Source memory**: `feedback_integration_claims_need_grep_not_assertion.md`.

**Pressure-test**: `litellm-platform-integration` iter-2 SCRUM. Plan asserted "AYIN federates from SigNoz over its existing HTTP client path — no AYIN code changes required" without verification. AYIN sibling grep returned ZERO matches. Federation did not exist. Forced honest Northstar P3 demotion to deferred status. Caught a class of false-witness invisible to iter-1 self-review because self-review never grepped the integration code.

**LÆX verdict**: RATIFY-AND-CODIFY, N=1 cross-session, convergent with Canon XXXV + §69 + Communication Covenant Rules 2+11. Biblical grounding: 2 Corinthians 13:1 (two witnesses for every word).

**Composition with existing canon**:
- Strengthens Canon XXXV (verbatim citation gate for decision-gating claims) — adds integration-claim corollary
- Extends §69 Citation Integrity Doctrine (tracking-artifact citation) — adds plan-body integration citation
- Operationalizes Communication Covenant Rule 2 (no false witness) for the integration-claim class
- Composes with Rule 11 (audit-pending disclosure) as the honest deferral mechanism

**No contradictions** with platform-canon / agents-playbook / architects-blueprint / operators-manual / security-guardrails / LASDLC-TEMPLATE-v1 / northstar.

---

## v3.11.0 — §78 LLM HTTP Dispatch Doctrine (2026-05-30, webshell-litellm-adapter ratification)

**Source**: /REFLECT session post webshell-litellm-adapter implementation. Three rules consolidated into one new top-level §78 covering streaming-by-default, provider unification, and the streaming-to-UX corollary.

### §78 — LLM HTTP Dispatch Doctrine (new section)
**Candidates**: /REFLECT proposals — promoted from `feedback_litellm_provider_unifier.md` (#3 in candidate doc) + `feedback_streaming_tx_forwarding.md` (#4). Streaming-by-default at the wire (§78.1) is **promotion of a previously-project-local CLAUDE.md rule** to canonical status — the policy existed in `lightarchitects-sdk/CLAUDE.md` but had never been ratified into canon.
**LÆX0-pattern verdict**: PROMOTE-WITH-RESHAPE. Author's original candidates #3 + #4 were independent; LÆX0 cold-context review folded them into a single doctrine where #3 (provider unifier) is the routing topology and #4 (tx-forwarding) is its streaming corollary. The new §78 therefore composes three rules under one canonical header rather than two scattered ones.
**Rationale**:
- The webshell-litellm-adapter session pressure-tested all three rules across `loops_demo` + `/chat` panel + `litellm_chat` SSE route (N=3 surfaces, 1 session). Before §78.3 forwarding: 10-15s silences in browser between phase markers. After: ~18ms token cadence end-to-end. The §78.2 unification eliminated the `OllamaCliProvider` dual-path 429/403 fallback logic from the affected surfaces — fallback complexity now lives in one proxy, not N surfaces.
- The "streaming-by-default" rule itself had been operationally enforced via project-local CLAUDE.md for several builds (hermes-litellm-integration earlier on 2026-05-30; webshell-la-native-backend prior). Canon promotion brings the rule into the corpus governed by Canon XII (Living Standard) and gives the Q-gate a citable reference instead of project-local CLAUDE.md.
- Operational implementation specifics (which OpenAI-compatible proxy, env names, config path) are explicitly **out of canon scope** and routed to Operators Manual. Canon holds the doctrine; the manual holds the implementation.
**Generality**: Applies to every LLM-touching surface the platform builds. Exception path is explicit (subprocess agent paths for tool-use stay direct, e.g. drawer's `lightarchitects_native`).
**Cross-canon ties**: Composes with §15.1 (structured log format for compliance audit) and Canon XL (Mixture-of-Experts Platform Architecture — §78 is how the MoE router speaks to its experts). No conflict with any other canon section.
**Companion memory entries**:
- `feedback_litellm_provider_unifier.md` (PROMOTED — provides implementation specifics, env contract)
- `feedback_streaming_tx_forwarding.md` (PROMOTED — pressure-test details)
- `feedback_ollama_v1_messages_anthropic_proxy.md` (operational — the failure mode §78.2 eliminates)
- `feedback_ollama_cloud_suffix_variants.md` (operational — Ollama-specific gotcha)
- `reference_ollama_no_cloud_disable.md` (operational reference)
**Promotion candidates held in memory (NOT promoted)**:
- `feedback_svelte5_push_then_mutate.md` — DEFERRED, pending N≥2 cross-session witness (LÆX0 verdict: 0.66 aggregate, N=1 insufficient)
- `feedback_svelte5_const_placement.md` — REJECTED for cookbook, kept in memory (LÆX0 verdict: compiler self-documents; 0.53 aggregate)
**Pressure-tested**: N=3 surfaces in 1 session (loops_demo, /chat, litellm_chat). LÆX0 aggregate scores: #3 principle 0.80, #4 corollary 0.75 — both clear ratification threshold.

---

## v3.10.0 — SSE Emission Existence — Rule S50.5c (2026-05-30, LÆX ratification)

**Source**: /REFLECT session post cockpit-wave-composer × ironclaw-autonomous-e2e cross-build plan audit. Caught pre-merge in plan cross-examination; would have been a silent production bug.

### §50.5c — Emission Existence (SSE and broadcast event boundaries)
**Candidate**: /REFLECT proposal L1 — promoted from `feedback_sse_emission_vs_type_declaration`
**LÆX verdict**: PROMOTE, confidence HIGH
**Rationale**: The emission-vs-declaration failure class is genuinely absent from existing canon. §48.2r covers TraceSpan emission for I/O (addition direction). §512 covers config-driven consumer audit (consumer direction, single-build scope). §50.5a/b cover shape and forward-compatibility. None cover producer-side callsite existence as distinct from type existence. A type can be declared in `types.rs`, compile cleanly, parse correctly in SSE deserializers, and pass all contract round-trip tests — while the emission callsite has been deleted in the same phase, leaving consumers receiving zero events at runtime. The failure is invisible to all automated gates; canon is exactly where rules invisible to tooling live.
**Generality**: Applies to any async pub/sub boundary — tokio broadcast channels, webhook pipelines, gRPC server streams, WebSocket message variants — wherever type registration and emission are separate operations that can diverge independently.
**Cross-build corollary**: ironclaw Phase 4 kept `EscalationEvent` type (line 730: "do NOT modify") while removing `WebEvent::Escalation` emission from `escalate_to_hitl()` (line 770: "Remove...emission"). cockpit-wave-composer Phase 4 badge filter watched `WebEvent::Escalation` — caught in plan cross-examination before build started.
**Cross-canon ties**: §50.5a (shape contract — not sufficient alone); §512 (consumer audit — inverse direction); Canon XLII (this entry is the narrative; §50.5c in cookbook is current state only).
**Pressure-tested**: N=1 (ironclaw × cockpit, 2026-05-30, pre-merge plan audit). LÆX judgment: promote at N=1 because the logical foundation is complete, the canon home is unambiguous, and the failure is invisible to all automated tooling.

---

## v3.9.0 — Loop-Strategy-Expansion LÆX ratifications (2026-05-29, XEA iter-5)

**Source**: LÆX Canon XXXIX ratification of /REFLECT proposals from `loop-strategy-expansion` plan-hardening session. Two PROMOTION-PENDING-KEVIN candidates approved for Cookbook insertion.

### §76 — Cross-Crate Type-Bridge Round-Trip Scoping
**Candidate**: #4 (queue #57) — promoted from `feedback_assert_eq_round_trip_isomorphism`
**Wave**: 4-lens REVIEW iter-4 / XEA iter-5 closure
**Rationale**: When two crates define types that bridge across a boundary (public facade re-export, lightweight mirror, serialization DTO, MCP wire format), the round-trip test `assert_eq!(A::from(B::from(a)), a)` fails deterministically when field sets are disjoint. `From` impls cannot synthesize information not carried in the source — fields present only in A are dropped on A→B and not restored on B→A. Worst-case failure mode: passes on trivial cases (default-valued fields), fails in CI when fixture data is populated. Fix: enumerate fields in both definitions; if disjoint, scope assertion to the shared invariant only and document why other fields are intentionally not preserved. Reference implementation: depth-only scoping for SDK ChainContext ↔ la-loops ChainContext (shared invariant: depth ≤ 7 per Canon §2.6).
**Cross-canon ties**: §70 (Type-Annotation Exhaustiveness — related but distinct: per-variant coverage vs round-trip preservation); §66 (Context Assembly Discipline — plausible vs correct framing applies)
**Pressure-tested**: 2026-05-29 `loop-strategy-expansion` §1.5 point 3 + shipped_means #7 originally specified full-equality round-trip; field-set disjointness caught by 4-lens REVIEW; corrected to depth-only assertion in iter-5. Awaiting Phase 5 implementation pressure-test for N=2.

### §77 — Pre-Allocated Span IDs for Child-Before-Parent Emission
**Candidate**: #5 (queue #58) — promoted from `feedback_preallocate_span_id_for_child_parent`
**Wave**: 4-lens REVIEW iter-4 / XEA iter-5 closure
**Rationale**: When a function emits child spans internally (e.g. mid-execution convergence checks) and the wrapping code creates the parent span AFTER the function returns, child spans have no parent.id at emission time. Default "wrap on exit" span construction breaks this. Fix: pre-allocate `span_id: Uuid = Uuid::new_v4()` in the caller, pass into inner function's StepContext, use same ID for both child .parent() references and the wrap span construction. UUIDs are 128-bit value types — no runtime cost. Anti-patterns covered: post-hoc parent assignment (mutable span storage incompatible with AYIN immutable ndjson append), channels (async sync overhead + ordering hazards), synthetic root spans (breaks trace tree for critical-path analysis).
**Cross-canon ties**: Aligns with observability-canon AYIN span schema (parent_id required for trace-tree reconstruction); aligns with W3C Trace Context spec (parent IDs intentionally pre-allocatable for distributed tracing); no contradiction with §64 (Serialized Git-Operations Mutex — different primitive)
**Pressure-tested**: 2026-05-29 `loop-strategy-expansion` §1.5 point 1 — convergence child spans inside `step()` initially specified `TraceContext::new().parent(step_span.id)` but `runner.rs` L299-307 creates step span AFTER `step()` returns. Plan corrected to pre-allocation pattern in iter-4. Awaiting Phase 3 implementation pressure-test for N=2.

**Companion canon updates this batch**: Blueprint §22.4 AMENDMENT + §22.4.1 NEW + §22.4.2 NEW (separate ratification — see architects-blueprint.CHANGELOG.md v3.6).

**LÆX queue housekeeping**: candidates #57 + #58 marked RATIFIED 2026-05-29; queue indices advanced to #59.

---

## v3.3.0 — Phase 7 ratifications (2026-05-18, iter-19)

**Source**: LÆX Phase 7 ratification walkthrough (see `LAEX-PHASE-7-QUEUE.md`).

### §57.11 — Northstar Pillar Mechanical Mapping
**Candidate**: #12 — promoted from `feedback_e2e_pillar_mechanical_validation`
**Wave**: 1 (Step-(d)-failure closure — memory marked RATIFIED 2026-05-17 but canon body was INCOMPLETE; pattern existed only in memory)
**Rationale**: For each Northstar Pillar a build claims to advance, Phase 7 E2E suite MUST declare ≥3 specific named scenarios validating that Pillar's mechanical promises. Generic "operator flow" coverage does not satisfy. Total E2E count = `3 × claimed_pillar_count + happy_path + perf_baseline + a11y_baseline`. Each scenario: specific name (E1, E2, ...) + specific mechanical promise (cite Pillar text) + specific assertion (≤500ms, framerate ≥30fps, terminal_window_open_count===0). Closes /GATE-7 N-gate from checkbox to empirical validation.
**Cross-canon ties**: §57 (E2E discipline), Northstar §S (pillar validation), Canon XXXII (E2E discipline)
**Pressure-tested**: `gitforest-live-ops` iter-7 had 6 generic categories; iter-8 expanded to 18 specific scenarios mapping to P1/P2/P4 with concrete assertions.

### §57.6d — Console-Error Zero Gate
**Candidate**: #33 — promoted from `feedback_comprehensive_e2e`
**Wave**: 6 RATIFY-WITH-MERGE (LOW conf, supervisor merge verdict — sub-rule of §57.6 rather than standalone section)
**Rationale**: All E2E test runs (Smoke/Capability/Integration) must terminate with zero console errors. §57.2b requires CAPTURE (artifact discipline); §57.6d requires ZERO (blocking gate). Capturing five errors and ignoring them is non-compliant. Allowlist permitted for benign-by-design messages. Implementation via Playwright `page.on('console')` + test teardown assertion.
**Cross-canon ties**: §57.2a (console.ndjson capture), §57.6 (stability tiers)
**Pressure-tested**: webshell-ui Playwright session (2026-05) — TypeError capture during comprehensive E2E catches hydration mismatches, event-handler closures, stale promise rejections that ship silently otherwise.

### §68 — Enum-Extension Collision Pre-check
**Candidate**: #32 — promoted from `feedback_enum_collision_precheck`
**Wave**: 6 RATIFY-AND-CODIFY (LOW conf but concrete pattern + decision-shaping + pressure-tested)
**Rationale**: Before any plan claims a new value in an existing enum, pre-check canon to verify position is free and count aligns. Common enums requiring pre-check: BuildViewMode, WebEvent, AgentDomain, Gate vocab [A+S+Q+C+O+P+K+D+T+R], LASDLC tier, status enums. Cross-plan coordination via helix coordination pact when two plans extend same enum.
**Cross-canon ties**: Blueprint Part VI (Compliance Matrix), webshell-api-surface §3.3 (current SOT), Canon XXXVIII (gate vocab)
**Pressure-tested**: `gitforest-live-ops` iter-7 API-canon audit caught view-mode-6 collision (pre-existing `comms` occupied that position); iter-8 allocated Wave Timeline as view-mode-7 + helix coordination pact.

---

> **Note on legacy version numbering** (2026-02 through 2026-05): Cookbook was historically versioned per-section rather than per-document. Version numbers `2.0.0`, `3.0.0`, and `1.6.0` each appear at multiple dates because section authors bumped section-scoped versions independently. From v3.2.1 (2026-05-18) forward, the cookbook follows strict per-document SemVer. Earlier entries below preserve original numbering for traceability.

---

## v3.2.1 — Git-Context Preamble (2026-05-18)

**Sections added**: §64.8 Git-Context Preamble (worker AgentRunner system prompt injection)
**Status**: LÆX Phase 7 ratification pending — **candidate #19**
**Authority**: operator-authorized Canon XV override (2026-05-18)

Closes operator-surfaced worker-git-awareness gap: workers received no explicit "you are in branch X, worktree Y, may only touch files Z" preamble, so context truncation or drift caused out-of-scope commits, forbidden git ops, or git2-bypassing-hook violations. §64.8 codifies the template that `wave_dispatcher` injects per task.

Composes with:
- LASDLC v2.5.3 `git_branching_invariants`
- agents-playbook §15.3.13 Pre-Dispatch Checklist
- `/BUILD` skill v2 Step 11.3.2 PT-7

---

## v3.2.0 — Git Mutex + Context Assembly + Concurrency Idioms (2026-05-18)

**Sections added**: §64 Serialized Git-Operations Mutex Pattern · §65 Builder Completeness Invariant · §66 Context Assembly Discipline (Plausible vs Correct) · §67 Concurrency Idioms (Rust async + git)
**Status**: LÆX Phase 7 ratification pending
**Authority**: operator-authorized Canon XV override (2026-05-18)
**Source**: ironclaw-architecture.html §9+§11+§15 cross-examination · ironclaw-spine SCRUM R1+R2+R3 convergence · Task#17 (Context7 gix/git2-rs/tokio) · Task#18 (git_routes.rs source verify)
**Promotion provenance**: 28 verification surfaces (5 R1–R5 + 7×3 SCRUM + 2 cross-exam)

Closes ironclaw §9 git-strategy + §11 context-assembly + §15 type-discipline canon gaps.

---

## v3.1.0 — Untrusted-Input Operational Patterns (2026-05-17)

**Sections added**: §63 Untrusted-Input Operational Patterns (P1–P4)
**Patterns**: build.rs ACE vector · structural arg parser · client-side diagram renderer strict mode (CWE-79 class) · symlink-before-canonicalize TOCTOU
**Schema**: each pattern carries uniform Threat / Vector / Mitigation / Lint / Test fields
**Cross-link**: Security-Guardrails §6.1.1 (Target-Repo Code Execution Surface)
**Source**: `architecture-intelligence-substrate` SCRUM Round 1 SERAPH adversarial review (2026-05-17); 4 patterns surfaced from BLOCKED-ON-CRITICAL verdict cleared via inline plan-fold + LÆX ratification batch

---

## v3.0.0 — Five-Star Engineering Targets (2026-05-12)

**Sections added**: §62 Five-Star Engineering Targets (absorbed from `five-star-engineering-targets.md`)
**Renaming**: "Canonical Six" → "Canonical Suite"
**Scope**: Canonical quality benchmark for all 9 engineering dimensions

*Numbering quirk: this is one of three v3.0.0 stamps in the legacy per-section versioning era. See note at top of file.*

---

## v3.0.0 — Quality-First Compression Sequencing (2026-05-04)

**Sections added**: §61 Quality-First Compression Sequencing (Canon XXXVI)
**Schema**: three-phase roadmap (Quality → Calibration → Compression) · 80/20 realistic-ceiling discipline · auto-decision precondition triple (P1 mechanical Northstar + P2 ≥3 calibrated examples + P3 ≥95% with citations) · categorical exclusion zones · fail-open-to-HITL contract
**Composition**: with XXXIII/XXXIV/XXXV — orders them into the only sequence that compresses without compounding error

*Numbering quirk: see top-of-file note.*

---

## v2.9.2 — INSUFFICIENT_EVIDENCE Aggregate Rule (2026-05-04)

**Sections added**: §60.10 INSUFFICIENT_EVIDENCE aggregate-reconciliation rule
**Rule**: Components with `INSUFFICIENT_EVIDENCE` + (floor<30 OR ≥50% sub-IE) are treated as N/A-equivalent in canonical weighted aggregate; dual reading required (canonical + with-IE-as-point)
**Source**: LDB v1.0 N=1 self-bootstrap on LASDLC template surfaced 74-vs-87 ambiguity that this rule canonizes
**Composes**: §58 (self-validation ceiling), §59 (interval reporting), §60 (threshold gate), §60.9 (inline citations)

---

## v2.9.1 — Inline Citations + IEEE Format (2026-05-04)

**Sections added**: §60.9 Inline Citations + IEEE Format
**Scope**: Architectural / design / algorithm / empirical / security / performance / standards-compliance decisions require inline `[N]` references backed by a `references:` block
**Format**: IEEE adapted with internal-source URI schemes (`canon://`, `cookbook://`, `lasdlc://`, `helix://`, `memory://`, `rubric://`, `test://`, `file://`)
**Cache substrate**: Firecrawl + Context7 cache at `<build_root>/.context/` for durable hydration across sessions/compactions
**Discipline**: Re-scrape decision logic + auditable `.meta.json` sidecars

---

## v2.9.0 — Confidence Threshold Gates (2026-05-04)

**Sections added**: §60 Confidence Threshold Gates (Canon XXXV)
**Thresholds**: Required ≥95%, preferred ≥99.99%
**Rule**: Confidence measured ONLY by verbatim primary-source citation; no primary source → UNVALIDATED → research mandatory via Tier 1–4 escalation (local → library → web → sibling)
**Gate**: Interval FLOOR gates the decision, not the point
**Composes**: §58 (self-validation ceiling) + §59 (interval reporting) — wide self-validated intervals correctly land below threshold and force research, not aspirational ship

---

## v2.8.0 — Self-Validation Ceiling + Confidence Intervals (2026-05-04)

**Sections added**: §58 Self-Validation Ceiling Operations (Canon XXXIII) · §59 Confidence Interval Reporting (Canon XXXIV)
**Self-validation ceiling**: structural ~70–75% on declarative work; independent verification (cold-context Explore agent) catches remaining ~30% incl. CRITICAL defects
**Interval reporting**: Confidence intervals beat points for evolving evaluations; self-validated reports MUST carry intervals ≥20pp wide. Corollary: prior pass's interval does not necessarily bracket future pass's point — each pass produces its own bracketing interval as evidence updates
**Source**: LASDLC template v2.0.0 → v2.0.4 cycle 5-pass cross-validation, 23pp self-bias measured (75% self - 52% independent at same template state), 26pp v4-onwards point swing

---

## v2.7.0 — E2E Test Engineering Standards (2026-05-01)

**Sections added**: §57 E2E Test Engineering Standards (Canon XXXII)
**Scope**: Capability-scoped specs · five-question artifact contract · EvidenceCollector correction loop · AYIN observability integration
**Source**: lightarchitects-webshell-ui test suite audit surfacing 300 blocked serial tests, 13+ stale route refs across 4,656 lines, zero diagnostic artifacts on failure

---

## v2.6.0 — Deliberate Live Playwright Cycle (2026-04-20)

**Sections added**: §56 Deliberate Live Playwright Evaluation Cycle (Canon XXXI)
**Pattern**: One persistent window · four-layer per-action evaluation (UI + network + backend logs + synthesis)
**Source**: lightarchitects-webshell copilot drawer session that surfaced Neo4j outage, missing gateway binary, slow Cypher query, and WebGL framebuffer bug invisible to the spec test suite

---

## v2.5.0 — Extend-Before-Add Gate Heuristic (2026-04-13)

**Sections added**: §55 Extend-Before-Add Gate Mosaic Expansion Heuristic
**Pairing**: Operational complement to Canon XXX (Strand Mosaic Completeness)
**Source**: unified-forging-vault Phase 0→1 gate ratification

§55 asserts **parsimony** (new gate only when orthogonal); Canon XXX asserts **completeness** (every strand has a home).

---

## v2.4.0 — Test Pyramid + SDK Patterns + Build Plan Template (2026-04-10)

**Sections added**: §52 Complete Test Pyramid Standard (Canon XXIX) · §53 SDK Type Patterns · §54 Build Plan Template Standard
**Scope**: Execution spine types · LongMemEval-validated retrieval patterns · CORSO template v2.0 · Platform architecture v2 updated with sections 11-13

---

## v2.0.0 — Boundary Sanitization Doctrine (2026-04-07)

**Sections added**: §51 Boundary Sanitization Doctrine (Canon XXVIII)
**Scope**: mandatory sanitization at every trust boundary crossing in agentic systems · 6-stage canonical pipeline · sanitization audit rule · multi-model trust boundary extension
**Source**: lÆx0-cli BCRA where 3/5 SQUAD agents independently flagged the same missing boundary, proving the need for a systematic mandate

*Numbering quirk: see top-of-file note.*

---

## v1.9.0 — Full-Stack Testing Doctrine (2026-04-06)

**Sections added**: §50 Full-Stack Testing Doctrine (Canon XXVII)
**Scope**: six required test suite types · E2E wiring confirmation rule · adversarial test requirements · known gap promotion protocol · contract test patterns · idempotency rules · tech-specific implementation guides
**Source**: lÆx0-cli Phase 9-10 where 1,189 tests at AMBER security score revealed the gap between component coverage and adversarial production confidence

---

## v1.8.0 — Acceptance Testing Doctrine (2026-04-05)

**Sections added**: §49 Acceptance Testing Doctrine — smoke tests (Tier 1.5) + HITL test suite (Tier 2) for every build plan phase
**Source**: lÆx0-cli Phase 9 where 5 parallel agents built components without acceptance tests, requiring full test suite parsing to verify each component

---

## v1.7.0 — Agent Post-Edit Gate Protocol (2026-04-05)

**Sections added**: §48 Agent Post-Edit Gate Protocol (Canon XXVI) — 3-tier quality/security/architecture gates for multi-agent engineering
**Source**: lÆx0-cli Phase 5-7 where SQUAD agents shipped code with 8 clippy errors, 92+ formatting diffs, and missing security annotations that individual agents didn't catch

---

## v1.6.0 — Publication Quality Standard (2026-03-28)

**Sections added**: §47 Publication Quality Standard (Canon XXII) · references AI Detection Checklist

*Numbering quirk: this 1.6.0 stamp is distinct from the 1.6.0 (2026-03-10) below, which covered §39 Identity Design Standards. See top-of-file note.*

---

## v3.0.0 — Constitutional Engineering Standards (2026-03-24)

**Sections added**: §46 Constitutional Engineering Standards
**Source**: adopted from Anthropic's Claude Constitution (CC0 licensed) and adapted for engineering agents
**Subsections**: §46.1 Seven Pillars of Honesty · §46.2 Cost-Benefit Harm Analysis · §46.3 Principal Hierarchy · §46.4 Corrigibility Spectrum · §46.5 Hard Constraints
**Cross-reference**: Light Architects Canon V–XVII
**Build**: platform-design-session-2026-03-24

*Numbering quirk: see top-of-file note.*

---

## v2.3.0 — MVT + Verification Consolidation (2026-03-21)

**Sections added**: §1.9 MVT Protocol (from `mvt-protocol.md`) · §1.10 Verification Before Recommendation (from `verification-protocol.md` + `lessons-learned.md`)
**Files deleted**: 5 superseded files — `coding-guidelines.md`, `gold-standard-planning-framework.md`, `mvt-protocol.md`, `verification-protocol.md`, `lessons-learned.md`, `parallel-execution-policy.md`

---

## v2.2.0 — §44–45 Cloud GPU Major Rewrite (2026-03-21)

**Sections expanded**: §44 7 → 11 subsections (added transformers version windows (44.2), non-standard architecture trap with evidence table (44.3), base model selection matrix (44.4), logging intervals (44.9), DeciLM-specific notes (44.10), post-training checklist (44.11)) · §45 added RunPod-specific notes (45.5)
**Merged**: All 14 rules from `training-playbook.md`
**Evidence**: 3 models (Nemotron 49B, Qwen3.5-27B, GPT-OSS 20B) + 1 abandoned attempt (Hermes-4.3-36B)
**Build**: fierce-forging-exodus Phase 7

---

## v2.1.0 — Cloud GPU Training Initial (2026-03-21)

**Sections added**: §44 Cloud GPU Training Standards · §45 Cloud Resource Management
**Build**: fierce-forging-exodus Phase 7

---

## v2.0.0 — Major Update (2026-03-15)

**Preamble**: New, with Kevin's quality mandate

**New sections**:
- §1.8 Deployment Configuration as Code (builder-vs-operator gap from falcon pentest)
- §5.2b Next.js/Vercel Security Standards (CSP, CORS, headers, Clerk mode)
- §7.5–7.7 AI rules (Decision Token, Ask Don't Guess, Grounding Verification — from 2024 research)
- §12.6 Auth Provider Mode Verification
- §35 Plugin expansion (dynamic discovery, skill-reviewer gate from soul:coalesce)

**Part X Specialized Domains** (new):
- §40 Pentest Engagement Standards (asset discovery, scope governance, wrong-codebase lesson)
- §41 Training Data Format Standards (ROLE_MAP, custom tokens, AYIN-enriched ChatML, adaptive reasoning depth)
- §42 SDK Consolidation Patterns (absorption workflow, workspace design from LA-SDK)
- §43 Observability Standards (TraceSpan schema, pivot detection, cognitive phases from AYIN)

**Build**: precise-sharpening-quill

*Numbering quirk: see top-of-file note.*

---

## v1.6.0 — Identity Design Standards (2026-03-10)

**Sections added**: §39 Identity Design Standards (strand taxonomy, independence test, audit process)
**Source**: The Right to Choose squad meeting

*Numbering quirk: see top-of-file note (distinct from 1.6.0 of 2026-03-28).*

---

## v1.5.0 — Voice Design (2026-03-09)

**Sections added**: §38.3–38.7 voice design · multi-speaker dialogue · per-sibling voice registry

---

## v1.4.0 — Production TTS Workflow (2026-03-04)

**Sections added**: §38.2 production TTS workflow · `voices.toml` source-of-truth rule

---

## v1.3.0 — Platform Services (2026-02-28)

**Sections added**: Part IX: Platform Services with §38 Voice Production (ElevenLabs)

---

## v1.2.0 — Plugin Distribution (2026-02-22)

**Sections added**: S17.8 Plugin Distribution Pattern
**Updated**: S17.6 Build-Deploy Pattern

---

## v1.1.0 — CORSO Cookbook Promotion (2026-02-16)

**Scope**: Promoted 6 patterns from CORSO Cookbook to universal standards

---

## v1.0.0 — Initial Consolidation (2026-02-11)

**Source**: Consolidated from Coding Guidelines v4.2.0 + Gold Standard Planning Framework v2.0

*Prior versions maintained in superseded documents.*

---

## Conventions for future amendments (codified 2026-05-18)

1. **Schema file = current state only.** Section content lives in the cookbook; amendment narrative lives here.
2. **Per-document SemVer from v3.2.1 forward.** No more per-section version bumps. Each cookbook release increments the doc-level version.
3. **One CHANGELOG entry per version.** Header line: `## vX.Y.Z — Title (YYYY-MM-DD)`. Body: sections added, source build, canon reference, cross-doc composition, LÆX candidate ID, authority citation.
4. **No tail-amendment blocks in `builders-cookbook.md`.** Use the inline `*Builders Cookbook vX.Y.Z | updated YYYY-MM-DD with …*` one-line footer if a visible at-a-glance current-version stamp is wanted; full detail lands here.
5. **LÆX promotion candidates**: track candidate ID in this CHANGELOG until Phase 7 ratification, then update status from "pending" to "ratified".
6. **Numbering quirks preserved for legacy entries.** v1.6.0, v2.0.0, and v3.0.0 each appear at multiple dates in the legacy era — these are preserved verbatim for traceability. New entries follow strict per-doc SemVer.
