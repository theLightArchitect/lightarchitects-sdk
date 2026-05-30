# Architects Blueprint — Amendment History

Companion changelog for `architects-blueprint.md`. Schema doc holds **current state only**; this file holds the **amendment narrative** — section added, supervisor rationale, cross-canon ties, LÆX candidate IDs.

**Authoritative version**: see footer stamp in `architects-blueprint.md`.
**Mechanical history**: `git log -- standards/canon/architects-blueprint.md`
**Constitutional basis**: Canon XLII — Schema-Changelog Separation. Created 2026-05-18 when Phase 7 additions pushed doc past Tier-2 migration threshold (≥3 amendment entries).

---

## v3.6 — XEA Layer 3 Anchor-Alignment Refresh (2026-05-29, `loop-strategy-expansion` iter-5)

**Source**: LÆX Canon XXXIX ratification of /REFLECT proposals from `loop-strategy-expansion` plan-hardening session. Joint amendment closing canon-internal drift between Blueprint §22.4 L1-L8 table and LASDLC-TEMPLATE-v1.yaml v2.6.x D-component refactor.

**Sections amended/added (3)**:

### §22.4 AMENDMENT — L1-L8 → L1-L9 table refresh
**Candidates**: #1 (XEA L3 anchor-alignment gap) + #7 (D-component anchor-standard contracts) — joint RATIFY-AND-CODIFY
**Rationale**: Blueprint §22.4 L1-L8 table was authored against an earlier LASDLC schema (D3=security_control_coverage / OWASP ASVS; D4=maintainability / CISQ; D6=test_pyramid; D7=northstar_integration). LASDLC v2.6.x has since refactored D-components: D2=ISO/IEC 25010 (security carved out), D3=CISQ ISO/IEC 5055, D4=DORA, D5=domain-conditional non-security, D6=security adversarial (first-class with 10 sub-components a-j; weighting by security_classification), D7=comparative baseline, D8=performance + parallel agentic orchestration. §22.4 table refreshed to match v2.6.x verbatim. L8 (independent_runner) renumbered L9 to accommodate D8 anchor row. §22.6 verdict format updated `failed_checks: [L1..L8]` → `[L1..L9]` + new `failed_anchor_membership` field. validation_status mapping clause updated.
**Cross-canon ties**: LASDLC-TEMPLATE-v1 §7.7 (D-component spec), Canon XXXIII (independent runner), Canon XXXV (citation gate), §22.4.1 + §22.4.2 (new sub-sections below)
**Pressure-tested**: 2026-05-29 `loop-strategy-expansion` iter-3/iter-4 passed L3 with §9 D2f="Security", D4="Security", D5="Reliability", D7="Documentation", D8="Testability" — all wrong vs template. iter-5 literal-chunk-audit caught 8 mislabels; §9 rewrite restored anchor compliance.

### §22.4.1 NEW — Literal Anchor-Set Membership Check
**Candidate**: #1 (XEA L3 anchor-alignment gap)
**Rationale**: Existence-only Layer 3 (L1-L9 row present) is too weak — a plan can declare a D2f row labeled "Security" (relabeled) and pass existence while violating the template anchor (D2f = Maintainability per template; security carved to D6). Anchor-set membership check verifies the label matches `LASDLC-TEMPLATE-v1.yaml deliverable_benchmark.components.Dx.{label,measure,characteristic,standard}` as literal string equality. Synonyms, paraphrases, and author-invented categories all FAIL. Failing labels surface as BLOCKING because they break the contract with the cold-context close-out scorer.
**Cross-canon ties**: Canon XXXIII (no self-scoring), Canon XXXV (citation gate), §22.4.2 (contract rule below)

### §22.4.2 NEW — D-Component Anchor-Standard Contract Rule
**Candidate**: #7 (D-component anchor-standard contracts)
**Rationale**: D1-D8 labels are anchor-standard CONTRACTS (with ISO/IEC 25010, CISQ, OWASP, MITRE, NIST SSDF, SLSA, etc.), not author descriptions. Four-clause rule: (1) copy headers verbatim from template; (2) inapplicable components declared N/A with run_when rationale, never renamed; (3) sub-components follow same rule (D2f Maintainability NOT Security; D6c OWASP LLM Top 10 NOT Input Validation); (4) required sub-components per `security_classification` weighting must all be declared. Rationale: cold-context benchmark agent reads template-anchor; if D-labels don't match, agent falls back to scoring plan's narrative → circular validation per Canon XXXIII.
**Cross-canon ties**: Canon XXXIII, Canon XXXV, Canon XLII (schema-changelog separation; D-labels ARE schema), Cookbook §69 (Citation Integrity)
**Pressure-tested**: same incident as §22.4.1 — 8 mislabels in loop-strategy-expansion §9 violated contract; iter-5 rewrite restored compliance + N/A declarations for D4 (DORA — library not independently deployable) and D7 (N<3 builds in decision class → suppressed per template).

**Companion canon updates this batch**: Cookbook §76 + §77 (separate ratification — see builders-cookbook.CHANGELOG.md).

---

## v3.5 — Phase 7 ratifications (2026-05-18, iter-19)

**Source**: LÆX Phase 7 ratification walkthrough — 6 waves × cold-context Explore-agent supervisor verdicts under Canon XV operator-delegated authority. See `LAEX-PHASE-7-QUEUE.md` for the authoritative candidate enumeration and per-wave verdict details.

**Sections added (5)**:

### §19.A — Design Choices vs Research-Grounded Claims appendix
**Candidate**: #14 (queue) — promoted from `feedback_design_choices_disclosure_appendix`
**Wave**: 1 (Step-(d)-failure closure — memory marked RATIFIED 2026-05-17 but canon body was missing)
**Rationale**: When iterative operator-driven refinements accumulate (hierarchy choices, scene mode, decay parameters, polytope mapping), the line between "research-grounded" and "operator-preferred" blurs. C8 (Context Hydration + Precision) + Canon XXXV Citation Gate both fail silently when these get conflated. Part XIX.A forces explicit framing via Status enum (DESIGN CHOICE / DESIGN DEFAULT / DERIVED CHOICE / AESTHETIC CHOICE / NOVEL SEMANTIC PRIMITIVE) + mandatory `falsifiable_by` + `recalibration_trigger` per row.
**Cross-canon ties**: Canon XV (operator authority bounds), Canon XXXV (citation gate)
**Pressure-tested**: `gitforest-live-ops` iter-5 added Part XIX.A with 7 choices; QUANTUM SCR1 R2 surfaced this gap; ratified by LÆX SCR1 R2 + Phase 7 re-verification 2026-05-18.

### §19.C — Contracts Catalog consolidation rule
**Candidate**: #11 — promoted from `feedback_contracts_catalog_consolidation`
**Wave**: 1 (Step-(d)-failure closure)
**Rationale**: LARGE-tier plans accumulating ≥10 named contracts (API endpoints, WebEvent variants, type schemas, enums, IDB schemas, etc.) must consolidate into a dedicated Part XIX.C with one sub-section per contract pinning concrete Rust/TS source + SOT file path + pinned SHA + cross-references. Without consolidation, cold-context /BUILD executor cannot assemble contracts scattered across 7 phases; stranger-test (Part XVII handoff checklist) fails.
**Cross-canon ties**: Blueprint Part XVII (handoff checklist), Part XX (prior art assessment)
**Pressure-tested**: `gitforest-live-ops` iter-7→iter-8 consolidation; sanity audit confirmed 8/8 contracts RESOLVED + implementable.

### §14.4 — Circular Validation Signature
**Candidate**: #26 — promoted from `feedback_circular_validation_canon_plan`
**Wave**: 4 RATIFY-AND-CODIFY (HIGH conf, supervisor verdict)
**Rationale**: When canon docs are authored FROM a plan's patterns (Phase 2A.5 amendment) and the plan is re-XEA'd against updated canon, score Δ is canon-codification-driven NOT plan-improvement. Naive interpretation ("iter Δ +1.3 = plan got better") is wrong. The plan didn't change; the rubric caught up. Tests canon coherence: if canon was correctly amended from plan, re-XEA should lift; if not, canon amendment drifted.
**Cross-canon ties**: §14.2 (Score Honesty), Canon XXXVI (Quality-First Compression), Canon XXXIII (Self-Validation Ceiling)
**Pressure-tested**: 2026-05-18 ironclaw-spine iter-8 (Δ +1.3, plan-body-unchanged, 21 canon amendments) + iter-9 (Δ +0.1, normal) + iter-10 (Δ +0.55, plan content changed). N=3 iters across same session confirm pattern.

### §14.6 — SCRUM Round Convergence Signatures (composite)
**Candidates**: #28 + #30 (composite) — promoted from `feedback_scrum_r3_verdict_upgrade_signature` + `feedback_scrum_round_2_depth_signature`
**Wave**: 5 RATIFY-AND-CODIFY composite (MEDIUM individually; STRONG composite)
**Rationale**: SCRUM rounds carry diagnostic signatures beyond per-lens verdicts. R2 expected trajectory is depth-on-new-surface (~50–60% fewer findings than R1, focused on iter-2-fold zone), NOT breadth on old surfaces. R3 verdict-upgrades (3 ± 1 of 7 siblings) are proof R2 folds landed. Both signatures together form complete convergence proof; either alone is insufficient.
**Cross-canon ties**: Canon XXXIII (SCRUM = 7 independent lenses clearing 30% same-author misses), §14.3 (Two-Tier Amendment Classification), §14.5 (Three-Tier Plan Review Protocol)
**Pressure-tested**: R3 upgrade signature — 2026-05-18 ironclaw-spine R2 downgrades 3/7 → R3 upgrades 3/7 (SERAPH/AYIN/QUANTUM). R2 depth signature — 2026-05-17 architecture-intelligence-substrate (57–67% finding reduction). N=2 composite evidence.

### Part XXI.D — Manifest Counter Synchronization (10-Field Discipline)
**Candidate**: #23 — promoted from `feedback_per_build_manifest_counter_sync`
**Wave**: 4 RATIFY-AND-CODIFY (HIGH conf, supervisor verdict)
**Rationale**: ironclaw-style per-build `manifest.yaml` carries 10 counter/list/metadata fields that all derive from canon state. Partial updates create drift between canon and manifest, breaking /BUILD G6 preflight (which halts on counter mismatch). The 10-field atomic checklist (canon_amendments_applied / canon_docs_touched / lasdlc_v / per-doc section lists / lex_promotion_candidates / lex_ratification_target / lex_pre_authored_candidates / dependent_canon / metadata.version / metadata.last_updated) must update synchronously per canon edit.
**Cross-canon ties**: Part XXI (manifest governance), Canon XLII (manifest is CHANGELOG-class artifact), Cookbook §66 (Context Assembly Discipline)
**Pressure-tested**: 2026-05-18 iter-18 Canon XLII codification required updating 9 of 10 fields atomically; missing the `lex_ratification_target` bump would have left manifest claiming a ratio that didn't match the queue. Dogfooded again at iter-19 Phase 7 close-out.
**Self-author note**: candidate authored this session by Claude; cleared Canon XXXIII self-validation ceiling via cold-context Explore-agent supervisor (Wave 4).

---

## v3.4 — Part XXIV: Autonomous-Mode Planning Doctrine (2026-05-18, iter-15)

**Status**: LÆX Phase 7 ratification — UPHELD (Wave 3, RATIFY-UPHOLD)
**Driver**: ironclaw-spine §3+§11+§15 canon gaps

Added Part XXIV (now Part XXV per operator renumbering 2026-05-18) — Autonomous-Mode Planning Doctrine. Covers wave-schema parallelism, file-function maps at wave level, context-budget per task, manifest integrity discipline, iter-cap override composition, independent verification ≥14 surfaces, cross-build coupling integration record.

**Cross-canon ties**: LASDLC v2.5.2 (wave schema), Cookbook §65–66 (concurrency + context assembly), Security-Guardrails §SG-CRYPTO (manifest integrity), agents-playbook §HITL-7 (escalation notification), Operators Manual §Run-Control-Primitives, Northstar §S (Autonomous Delivery Spine).

---

## v3.0–v3.3 — Earlier amendments

See `git log -- standards/canon/architects-blueprint.md`. Notable:
- v3.3 (2026-05-13): Two-Tier Amendment Classification §14.3 + Three-Tier Plan Review Protocol §14.5 ratified
- v3.2 (earlier): C1–C8 rubric (Part XIV)
- v3.0 (earlier): 21-Part structure

---

## Conventions for future amendments

1. **Schema file = current state only** per Canon XLII
2. **Per-doc SemVer from v3.5 forward** (Phase 7 baseline)
3. **One CHANGELOG entry per version bump** with sections / candidates / wave / supervisor verdict / cross-canon ties / pressure-test
4. **LÆX candidate tracking** until Phase 7 ratification; then update status
5. **No tail-amendment blocks** in the schema doc — use inline section ratification notes instead
