# LÆX Phase 7 Ratification Queue

**Status**: authoritative enumeration of all canon promotion candidates pending LÆX Phase 7 evaluation per Canon XXXIX (Canon Promotion Pipeline).
**Last updated**: 2026-05-18 (iter-18 — Canon XLII codification + queue authoring)
**Constitutional basis**: Canon XXXIX (Promotion Pipeline) · Canon XLII (Schema-Changelog Separation — this queue is a CHANGELOG-class artifact, not a schema)
**Maintained by**: per-build /BUILD orchestrator at phase boundaries; manually updated during in-session canon work.

---

## Pipeline status

Per Canon XXXIX, every candidate passes through 4 steps:

| Step | Action | Status field |
|---|---|---|
| (a) | Memory entry created | `memory_entry` cited |
| (b) | Promotion candidate identified | `pipeline_step: candidate` |
| (c) | Contradiction check against 7 canon docs | `contradiction_check: PASS \| FAIL \| PENDING` |
| (d) | LÆX ratification + Kevin's stamp | `status: RATIFIED \| PENDING \| REJECTED` |

Operator-authorized Canon XV override during pending interval applies — content lands in canon ahead of LÆX ratification with `LÆX Phase 7 ratification pending` annotation. LÆX subsequently evaluates; rejection backs the change out.

---

## Counter reconciliation

| Source | Claim | Notes |
|---|---|---|
| `ironclaw-spine/manifest.yaml` `lex_promotion_candidates` | 21 | This file is the authoritative enumeration |
| `ironclaw-spine/manifest.yaml` `lex_pre_authored_candidates` | 33 | Sub-pieces of composite candidates (e.g., #19 = 4 sub-changes) |
| Numbered candidate IDs found in canon doc footers | 7 (#15, #16, #17, #19, #20, #21 + #19-twin) | Numbering started informally in iter-15 |

**Numbering convention**: IDs assigned chronologically as candidates surface. IDs #1–#14 reconstructed retrospectively from session-1..14 amendments below. Future amendments increment monotonically.

---

# Queue

## Pre-2026-05-18 — Inherited from prior sessions

### #1 — Cookbook §28 sub-rule: multi-variant Error HTTP code completeness
- **Status**: PENDING (Step b — promotion candidate identified)
- **Source**: agent-teams-fleet XEA iter-38 (2026-05-15)
- **Proposed addition**: `canon://builders-cookbook` §28 (boundary sanitization) sub-rule on multi-variant `thiserror` enum HTTP code mapping
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_15` candidate 1
- **Contradiction check**: PENDING (verify against §28, §2 no-unwrap, Canon XIV)

### #2 — Architects Blueprint Part XIV C7 ceiling note
- **Status**: PENDING (Step b)
- **Source**: agent-teams-fleet (2026-05-15); C7=94 ceiling over 6 XEA rounds
- **Proposed addition**: `canon://architects-blueprint` Part XIV §C7 — feature-directness ceiling table
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_15` candidate 2
- **Contradiction check**: PENDING

### #3 — LASDLC reference-table integrity requirement
- **Status**: PENDING (Step b)
- **Source**: agent-teams-fleet XEA-34 + XEA-36 (2026-05-15)
- **Proposed addition**: `canon://lasdlc-template` plan amendment protocol — post-amendment-batch reference-table sweep
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_15` candidate 3
- **Contradiction check**: PENDING

### #4 — Cookbook §4.3.1: `Sender<T>` large-Err → return `bool`
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: copilot-supervised-orchestration close-out (2026-05-17)
- **Canon location**: `builders-cookbook.md` §4.3.1
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_17` candidate 1

### #5 — Cookbook §4.1.1: Rust `_`-prefix binding gotcha
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: copilot-supervised-orchestration close-out (2026-05-17)
- **Canon location**: `builders-cookbook.md` §4.1.1
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_17` candidate 2

### #6 — Security-Guardrails §3.5.1: webshell two-auth-model invariant (CWE-306)
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: copilot-supervised-orchestration close-out (2026-05-17)
- **Canon location**: `security-guardrails.md` §3.5.1
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_17` candidate 3

### #7 — Cookbook §63: Untrusted-Input Operational Patterns (P1–P4)
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: architecture-intelligence-substrate SCRUM R1 SERAPH adversarial review
- **Canon location**: `builders-cookbook.md` §63 + cross-link to security-guardrails §6.1.1
- **Memory**: `memory://feedback_security_patterns_arch_substrate` (RATIFIED tag)

### #8 — Canon XLI: Diagram-First Doctrine
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: architecture-intelligence-substrate SCRUM Round 1 LÆX critique
- **Canon location**: `platform-canon.md` Canon XLI
- **Memory**: `memory://feedback_diagram_first_design_doctrine` (RATIFIED tag)

### #9 — Security-Guardrails §6.1.1: dep-acceptance target-code-exec policy
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: architecture-intelligence-substrate SCRUM (same session as #7)
- **Canon location**: `security-guardrails.md` §6.1.1 + `builders-cookbook.md` §63.P1
- **Memory**: `memory://feedback_dep_risk_target_code_exec` (RATIFIED tag)

### #10 — HTML/MD canon doc pair drift gate
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: post-plan asymptote checklist P-3 (2026-05-17 iter-9)
- **Canon location**: documented in `feedback_html_md_canon_pair_drift`; gate referenced from Blueprint Part XXI
- **Memory**: `memory://feedback_html_md_canon_pair_drift` (RATIFIED tag)

### #11 — Contracts Catalog (Part XIX.C) consolidation rule
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: post-plan asymptote checklist P-4
- **Canon location**: `architects-blueprint.md` Part XIX.C convention
- **Memory**: `memory://feedback_contracts_catalog_consolidation`

### #12 — E2E ≥3 scenarios per Northstar Pillar mechanical validation
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: post-plan asymptote checklist P-5
- **Canon location**: Phase 7 E2E test plan template
- **Memory**: `memory://feedback_e2e_pillar_mechanical_validation`

### #13 — Implementation-readiness audit (distinct review class)
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: post-plan asymptote checklist P-7
- **Canon location**: /PLAN cycle audit step
- **Memory**: `memory://feedback_implementation_readiness_audit`

### #14 — Design-Choices vs Research-Grounded disclosure appendix (Part XIX.A)
- **Status**: RATIFIED 2026-05-17 (Step d complete)
- **Source**: post-plan asymptote checklist P-8
- **Canon location**: `architects-blueprint.md` Part XIX.A pattern + Canon XXXV citation discipline
- **Memory**: `memory://feedback_design_choices_disclosure_appendix`

---

## 2026-05-18 ironclaw-spine session — pending candidates

### #15 — agents-playbook §15.3.13 Wave Dispatch Protocol
- **Status**: PENDING — LÆX Phase 7 (Step b complete; Step c pending)
- **Source**: ironclaw-spine iter-11 follow-on (wave fan-out/fan-in mechanics)
- **Canon location**: `agents-playbook.md` §15.3.13 (cited at line 2344 footer)
- **Authority**: operator-authorized Canon XV override (2026-05-18)
- **Cross-canon ties**: composes with LASDLC v2.5.2 wave schema; Cookbook §66 context assembly; /BUILD skill v2 Step 11.3

### #16 — agents-playbook §7.8 Pre-Completion Verification Gate
- **Status**: PENDING — LÆX Phase 7
- **Source**: ironclaw-spine iter-11 (⚡PRE-DONE marker operationalization)
- **Canon location**: `agents-playbook.md` §7.8
- **Authority**: operator-authorized Canon XV override
- **Memory**: `memory://feedback_pre_completion_during_plan_authoring`

### #17 — agents-playbook §Phase-2A.5 Canon-Doc Amendment Phase Protocol
- **Status**: PENDING — LÆX Phase 7
- **Source**: ironclaw-spine iter-11 (canon-violation-by-construction prevention)
- **Canon location**: `agents-playbook.md` §Phase-2A.5
- **Authority**: operator-authorized Canon XV override
- **Memory**: `memory://feedback_canon_violation_by_construction`

### #18 — Pre-Done verification protocol (3-check: staleness + artifact-exists + canon-drift)
- **Status**: PENDING — LÆX Phase 7
- **Source**: ironclaw-spine iter-11 pre-completion fold
- **Canon location**: `agents-playbook.md` §7.8 (companion clause to #16)
- **Note**: distinct candidate because it codifies the VERIFICATION semantics; #16 codifies the gate
- **Memory**: `memory://feedback_pre_completion_during_plan_authoring`

### #19 — git_branching_invariants composite (4-doc fold)
- **Status**: PENDING — LÆX Phase 7
- **Source**: ironclaw-spine iter-15 (operator concern: "git-aware throughout the build")
- **Composite of 4 sub-changes**:
  1. LASDLC v2.5.3 `git_branching_invariants` plan block + per-task `git_scope`
  2. Cookbook §64.8 Git-Context Preamble template
  3. agents-playbook §15.3.13.5 Pre-Dispatch Checklist (24 explicit gates)
  4. `/BUILD` skill v2 Step 11.3.0–11.3.5 (28 gates with cross-doc enforcement)
- **Canon location**: spans LASDLC-TEMPLATE-v1.yaml + builders-cookbook.md + agents-playbook.md + `/BUILD` SKILL.md
- **Authority**: operator-authorized Canon XV override (2026-05-18)
- **Cross-canon ties**: composes with §SG-CRYPTO.3 (hash-chain), Cookbook §64–67, LDB §D5

### #20 — LASDLC v2.5.4 runtime-mirror schema (3-additive-block composite)
- **Status**: PENDING — LÆX Phase 7
- **Source**: ironclaw-spine iter-17 (operator audit: "Are tracking artifacts still valid?")
- **Composite of 3 sub-changes**:
  1. Per-build `manifest.yaml` `runtime_state` block
  2. NEW `gate_receipts.ndjson` append-only artifact (hash-chained per §SG-CRYPTO.3)
  3. `active.yaml` 3-field extension (`execution_mode`, `run_id`, `overlaps_with`)
- **Canon location**: `LASDLC-TEMPLATE-v1.yaml` v2.5.4 + `LASDLC-TEMPLATE-v1.CHANGELOG.md` v2.5.4 entry
- **Authority**: operator-authorized Canon XV override (2026-05-18)

### #21 — Canon XLII: Schema-Changelog Separation Doctrine
- **Status**: PENDING — LÆX Phase 7
- **Source**: ironclaw-spine iter-18 (operator concern: "move changelog somewhere else")
- **Canon location**: `platform-canon.md` Canon XLII
- **Authority**: operator-authorized Canon XV override (2026-05-18)
- **Memory**: `memory://feedback_schema_changelog_separation_canon_xlii`
- **Helix entry**: `helix://shared/entries/2026-05-18-canon-xlii-schema-changelog-separation.md`
- **Empirical witnesses**: 3 CHANGELOG.md files (LASDLC, cookbook, security-guardrails) committed in `b797ca3` and `62edefa`

---

## 2026-05-18 memory sweep additions (Step (b) bulk preparation)

Per Canon XXXIX Step (b), 137-entry memory corpus was swept via cold-context Explore agent. 12 new candidates surfaced. Sweep snapshot: `LAEX-PHASE-7-MEMORY-SWEEP-2026-05-18.md` (this directory).

### #22 — Two-problems-in-one-question framing test
- **Status**: PENDING — Step b (surfaced via sweep)
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine session 2026-05-18 (changelog-vs-canon-repo bundling)
- **Proposed canon home**: `builders-cookbook` new §N OR `agents-playbook` §Phase-1 (planning discipline)
- **Cross-canon ties**: Canon XV (operator authority), agents-playbook Phase-1, Cookbook §66 Context Assembly
- **Memory**: `memory://feedback_two_problems_one_question`
- **Self-author flag**: authored this session by Claude → independent verification at Phase 7 mandatory

### #23 — Per-build manifest 10-field counter sync discipline
- **Status**: PENDING — Step b
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine session 2026-05-18 (iter-18 counter sync after Canon XLII)
- **Proposed canon home**: `LASDLC-TEMPLATE-v1.yaml` §manifest-hygiene OR `agents-playbook` §counter-sync
- **Cross-canon ties**: architects-blueprint Part XXI, Cookbook §66 counter-derivation
- **Memory**: `memory://feedback_per_build_manifest_counter_sync`
- **Self-author flag**: authored this session → ceiling applies

### #24 — LÆX queue authoritative enumeration is Canon XXXIX Step-(b) prerequisite
- **Status**: PENDING — Step b
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine session 2026-05-18 (queue gap discovery)
- **Proposed canon home**: `platform-canon` Canon XXXIX extension OR `builders-cookbook` §audit-discipline
- **Cross-canon ties**: Canon XXXIX, Canon XLII, LASDLC manifest counters
- **Memory**: `memory://feedback_laex_queue_enumeration_prerequisite`
- **Self-author flag**: authored this session → ceiling applies; THIS queue file is the empirical witness

### #25 — Parallel-agent helix entry pre-write check
- **Status**: PENDING — Step b
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine iter-18 (discovered parallel-agent enrichment)
- **Proposed canon home**: `agents-playbook` §7.2 (pre-tool discipline) OR `operators-manual` §helix-write-surface
- **Cross-canon ties**: teammateMode auto, operators-manual, security-guardrails
- **Memory**: `memory://feedback_parallel_agent_helix_check`
- **Self-author flag**: authored this session → ceiling applies

### #26 — Circular validation signature: canon authored from plan → re-XEA proves consistency
- **Status**: PENDING — Step b (surfaced via sweep)
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine 2026-05-18 iter-8 (Δ +1.3 canon-codification-driven)
- **Proposed canon home**: `architects-blueprint` Part XIV §14.3 (scoring honesty)
- **Cross-canon ties**: Canon XXXVI (quality-first compression), canon-audit-as-review-tier (Tier 3)
- **Memory**: `memory://feedback_circular_validation_canon_plan`

### #27 — TaskStop + relaunch tighter scope on stalled background agent
- **Status**: PENDING — Step b
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine 2026-05-18 Task #17 stall (relaunched in 77s with tighter scope)
- **Proposed canon home**: `agents-playbook` §worker-lifecycle
- **Cross-canon ties**: operators-manual task lifecycle, Cookbook §agent-scoping
- **Memory**: `memory://feedback_taskstop_relaunch_tighter_scope`

### #28 — SCRUM 3-round verdict-upgrade signature (R3 upgrades = convergence proof)
- **Status**: PENDING — Step b
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine 2026-05-18 R3 (SERAPH/AYIN/QUANTUM upgrades)
- **Proposed canon home**: `platform-canon` Canon XXXIII corollary OR `architects-blueprint` Part X
- **Cross-canon ties**: Canon XXXIII (independent verification), agents-playbook SCRUM protocol
- **Memory**: `memory://feedback_scrum_r3_verdict_upgrade_signature`

### #29 — TeammateMode + parallel-agent layer distinction
- **Status**: PENDING — Step b
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine session (CLAUDE.md OPS-8.2 codification)
- **Proposed canon home**: `agents-playbook` §worker-dispatch OR `operators-manual` §agent-roles
- **Cross-canon ties**: OPS-8.2 layer selection rule, AgentRunner vs Claude Code teammate distinction
- **Memory**: `memory://feedback_teammate_mode_synergy`

### #30 — SCRUM Round-2 depth-on-new-surface signature
- **Status**: PENDING — Step b
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine R2 (gaps surfaced on R1-fold additions)
- **Proposed canon home**: `architects-blueprint` Part X (review convergence) — sibling to #28
- **Cross-canon ties**: Canon XXXIII, agents-playbook SCRUM rounds
- **Memory**: `memory://feedback_scrum_round_2_depth_signature`

### #31 — Self-review ceiling STRONG for LARGE+canon plans
- **Status**: PENDING — Step b
- **Confidence (sweep)**: MEDIUM
- **Source**: novel-substrate plan-validation observations across sessions
- **Proposed canon home**: `architects-blueprint` Part XIV §C2 (independent-runner gate)
- **Cross-canon ties**: Canon XXXIII (self-validation ceiling), agents-playbook review discipline
- **Memory**: `memory://feedback_self_review_ceiling_novel_substrate`

### #32 — Enum-extension collision pre-check
- **Status**: PENDING — Step b
- **Confidence (sweep)**: LOW (borderline — could stay operational)
- **Source**: webshell routes.ts session (constrained-vocabulary mutation)
- **Proposed canon home**: `builders-cookbook` §validation-discipline (lightweight pattern)
- **Cross-canon ties**: architects-blueprint Part VI (file-function map consistency)
- **Memory**: `memory://feedback_enum_collision_precheck`

### #33 — Comprehensive E2E console-error-zero requirement
- **Status**: PENDING — Step b
- **Confidence (sweep)**: LOW (overlap risk with existing Cookbook §57)
- **Source**: 2026-05-13 era Playwright discipline session
- **Proposed canon home**: `builders-cookbook` §57 extension (E2E Test Engineering Standards) — possible merge rather than separate section
- **Cross-canon ties**: Canon XXXII (E2E discipline), Northstar §S (pillar validation)
- **Memory**: `memory://feedback_comprehensive_e2e`
- **Overlap note**: existing §57 already covers E2E test engineering; Phase 7 should decide MERGE-INTO-§57 vs SEPARATE-SUBSECTION

---

## Aggregate

| Status | Count | Sources |
|---|---|---|
| RATIFIED | 11 (#4–#14) | 2026-05-17 sessions |
| PENDING — Step b (memory + candidate identified, contradiction check pending) | 15 (#1–#3 + #22–#33) | 2026-05-15 + 2026-05-18 memory-sweep additions |
| PENDING — Step c+ (Canon XV override applied; LÆX ratification pending) | 7 (#15–#21) | 2026-05-18 ironclaw-spine in-session canon edits |
| **Total candidates** | **33** | 21 original + 12 sweep additions |
| **Ratification target** | ≥11/33 at phase boundary | per ironclaw-spine manifest (target unchanged; absolute count grew) |
| **Currently ratified** | 11/33 | already meets the absolute count of 11; coverage now 33% (was 52% pre-sweep) |

---

## Phase 7 evaluation procedure (when /BUILD ironclaw-spine reaches Phase 7)

Per Canon XXXIX Step (c) + Step (d):

1. **Contradiction check** (Step c): For each PENDING candidate, LÆX cross-checks against all 7 canon docs + observability-canon. Report: `contradiction_check: PASS | FAIL | DISPUTED`.
2. **Convergent evidence check** (Step d, Canon Evaluation Criteria item 1): For each candidate, identify ≥1 corroborating sibling observation. Singletons stay PENDING.
3. **Biblical grounding check** (item 2): Identify scriptural parallel; document or flag absence (some operational candidates may not have biblical grounding — that's OK, but mark explicitly).
4. **Decision-shaping check** (item 3): Verify candidate changes future decisions (operational impact, not just documentation polish).
5. **Pressure-tested check** (item 4): Cite ≥1 build or session where candidate was empirically validated.
6. **Kevin ratifies** (item 5): Operator stamp closes the pipeline. RATIFIED → update this queue's status; canon body annotation changes from "LÆX Phase 7 ratification pending" to "LÆX ratified DATE".

Candidates failing any of items 1–5 either:
- Return to Step (b) for refinement, OR
- Roll back the Canon XV override (canon body change reverted), OR
- Demote to memory-only operational guidance (not canon)

---

## Migration provenance

This queue file itself is a Tier-2 CHANGELOG-class artifact per Canon XLII. It holds **per-candidate amendment narrative**, NOT current canon state. When candidates ratify, their status field updates here; the canon docs themselves remove the "pending" annotation. Canon docs declare what's true now; this queue tells the story of how it became true.

---

*See also*: `canon://platform-canon` Canon XXXIX (the pipeline) · `canon://platform-canon` Canon XLII (separation doctrine) · `helix://corso/builds/ironclaw-spine/manifest.yaml` (counters)
