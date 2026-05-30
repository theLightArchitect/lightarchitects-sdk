# LÆX Phase 7 Ratification Queue

**Status**: authoritative enumeration of all canon promotion candidates pending LÆX Phase 7 evaluation per Canon XXXIX (Canon Promotion Pipeline).
**Last updated**: 2026-05-23 (Canon N1B #47 RATIFIED prior session; #48-#52 RATIFIED 2026-05-23 via Canon XV, Kevin Francis Tan — batch application complete this session)
**Total candidates**: 56 (was 55; +1 this session: #56 frontend HITL surface implementation contract — PENDING Kevin's stamp 2026-05-30).
**Ratification status 2026-05-22**: #44–#46 PENDING (GATE skill plugin amendments; require plugin PR + LÆX co-sign). SG-1–SG-4 applied under Kevin's Canon XV stamp — `canon_location_prelisted: security-guardrails.md §3.3/§3.5/§3.5.2/§5.5`.
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

### #1 — Cookbook §15.8 Multi-Variant Handler Error Coverage
- **Status**: SUPERSEDED_BY_PROVISIONAL_CANON at Phase 7 2026-05-18 (Step d closed via duplicate-finding)
- **Source**: agent-teams-fleet XEA iter-38 (2026-05-15)
- **Canon location**: `builders-cookbook.md` §15.8 Multi-Variant Handler Error Coverage (line 1874) — already exists as PROVISIONALLY_VALID
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_15` candidate 1
- **Phase-7 supervisor verdict**: REJECT (duplicate of existing canon). Canon §15.8 was authored under Canon XV operator-authorized override during the 2026-05-15 session — the memory entry documenting evidence and the canon section were both authored together, with status `PROVISIONALLY_VALID` (N=1, confidence {low: 82, point: 90, high: 96}). The 2026-05-18 queue archaeology surfaced the memory but missed the already-canonized section. **No further ratification action**. Monitor for N≥3 independent confirmations to elevate to VALIDATED.

### #2 — Architects Blueprint Part XIV §C7 ceiling note + Northstar §C7 heuristics
- **Status**: SUPERSEDED_BY_PROVISIONAL_CANON at Phase 7 2026-05-18 (Step d closed via duplicate-finding)
- **Source**: agent-teams-fleet (2026-05-15); C7=94 ceiling over 6 XEA rounds
- **Canon location**: `architects-blueprint.md` Part XIV §C7 ceiling observation (line 894) + score ceiling calibration table (lines 1264-1273) + `northstar.md` C7 ceiling heuristics table (line 574) — all already exist as PROVISIONALLY_VALID
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_15` candidate 2
- **Phase-7 supervisor verdict**: REJECT (duplicate of existing canon). PROVISIONALLY_VALID N=1 confidence interval {low: 88, point: 93, high: 97}. **No further ratification action**. Monitor for N≥3 independent builds to confirm ceiling.

### #3 — LASDLC reference-table integrity requirement
- **Status**: SUPERSEDED_BY_PROVISIONAL_CANON at Phase 7 2026-05-18 (Step d closed via duplicate-finding)
- **Source**: agent-teams-fleet XEA-34 + XEA-36 (2026-05-15)
- **Canon location**: `LASDLC-TEMPLATE-v1.yaml` `reference_table_integrity` block (line 7237) — already exists with description, sweep_trigger, high_drift_sections, sweep_procedure, gate_behavior; `added_in_template_version: "2.5.1"` sourced from same memory entry
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_15` candidate 3
- **Phase-7 supervisor verdict**: REJECT (duplicate of existing canon). **No further ratification action**.

### Wave 2 process finding — Step-(d)-before-Step-(b) ordering

All three Wave 2 candidates (#1, #2, #3) exhibited the same failure mode: memory entries created during the 2026-05-15 XEA session were promoted to formal ratification candidates on 2026-05-18, but the canon bodies they reference had ALREADY been committed during the same 2026-05-15 session under Canon XV operator-authorized override.

**Root cause**: When canon edits land under Canon XV operator-override (e.g., during in-session XEA folds), the memory entry documenting the evidence should be tagged with `canon_location_prelisted: <doc>#<section>` to prevent duplicative ratification-candidate promotion in subsequent queue archaeology.

**Process amendment proposal** (queue as candidate #34 for future Phase 7): when /BUILD applies Canon XV override at /PLAN time or amendment time, the memory entry write should carry `canon_location_prelisted` frontmatter field. The LÆX queue authoring process should grep for this field to skip already-canonized entries.

### #4 — Cookbook §4.3.1: `Sender<T>` large-Err → return `bool`
- **Status**: RATIFIED 2026-05-17; verified at Phase 7 2026-05-18 (Step d complete; canon body grep-confirmed)
- **Source**: copilot-supervised-orchestration close-out (2026-05-17)
- **Canon location**: `builders-cookbook.md` §4.3.1
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_17` candidate 1

### #5 — Cookbook §4.1.1: Rust `_`-prefix binding gotcha
- **Status**: RATIFIED 2026-05-17; verified at Phase 7 2026-05-18 (Step d complete; canon body grep-confirmed)
- **Source**: copilot-supervised-orchestration close-out (2026-05-17)
- **Canon location**: `builders-cookbook.md` §4.1.1
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_17` candidate 2

### #6 — Security-Guardrails §3.5.1: webshell two-auth-model invariant (CWE-306)
- **Status**: RATIFIED 2026-05-17; verified at Phase 7 2026-05-18 (Step d complete; canon body grep-confirmed)
- **Source**: copilot-supervised-orchestration close-out (2026-05-17)
- **Canon location**: `security-guardrails.md` §3.5.1
- **Memory**: `memory://project_canon_promotion_candidates_2026_05_17` candidate 3

### #7 — Cookbook §63: Untrusted-Input Operational Patterns (P1–P4)
- **Status**: RATIFIED 2026-05-17; verified at Phase 7 2026-05-18 (Step d complete; canon body grep-confirmed)
- **Source**: architecture-intelligence-substrate SCRUM R1 SERAPH adversarial review
- **Canon location**: `builders-cookbook.md` §63 + cross-link to security-guardrails §6.1.1
- **Memory**: `memory://feedback_security_patterns_arch_substrate` (RATIFIED tag)

### #8 — Canon XLI: Diagram-First Doctrine
- **Status**: RATIFIED 2026-05-17; verified at Phase 7 2026-05-18 (Step d complete; canon body grep-confirmed)
- **Source**: architecture-intelligence-substrate SCRUM Round 1 LÆX critique
- **Canon location**: `platform-canon.md` Canon XLI
- **Memory**: `memory://feedback_diagram_first_design_doctrine` (RATIFIED tag)

### #9 — Security-Guardrails §6.1.1: dep-acceptance target-code-exec policy
- **Status**: RATIFIED 2026-05-17; verified at Phase 7 2026-05-18 (Step d complete; canon body grep-confirmed)
- **Source**: architecture-intelligence-substrate SCRUM (same session as #7)
- **Canon location**: `security-guardrails.md` §6.1.1 + `builders-cookbook.md` §63.P1
- **Memory**: `memory://feedback_dep_risk_target_code_exec` (RATIFIED tag)

### #10 — HTML/MD canon doc pair drift gate
- **Status**: RATIFIED 2026-05-17 + verified at Phase 7 2026-05-18
- **Source**: post-plan asymptote checklist P-3 (2026-05-17 iter-9)
- **Canon location**: `architects-blueprint.md` Part XXIV §24.6 (corrected from queue's prior "Part XXI" claim per Wave 1 supervisor drift finding)
- **Memory**: `memory://feedback_html_md_canon_pair_drift` (RATIFIED tag)

### #11 — Contracts Catalog (Part XIX.C) consolidation rule
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d COMPLETED — section authored)
- **Source**: post-plan asymptote checklist P-4
- **Canon location**: `architects-blueprint.md` §19.C (codified 2026-05-18; Wave 1 supervisor flagged Part XIX.C as MISSING; canonized this session per Phase-7 ratification)
- **Memory**: `memory://feedback_contracts_catalog_consolidation`
- **Phase-7 note**: prior "RATIFIED 2026-05-17" claim was Step-(d) failure (memory marked ratified without canon body); supervisor caught drift; Phase 7 closed gap by authoring §19.C this session

### #12 — E2E ≥3 scenarios per Northstar Pillar mechanical validation
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d COMPLETED — section authored)
- **Source**: post-plan asymptote checklist P-5
- **Canon location**: `builders-cookbook.md` §57.11 Northstar Pillar Mechanical Mapping (codified 2026-05-18; Wave 1 supervisor flagged as INCOMPLETE — pattern existed only in memory; canonized this session)
- **Memory**: `memory://feedback_e2e_pillar_mechanical_validation`
- **Phase-7 note**: prior ratification was operational-pattern recognition without formal canon section; Phase 7 added §57.11 with 3 rules (S57.11a-c) + reference example + composition note

### #13 — Implementation-readiness audit (distinct review class)
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d COMPLETED — section authored)
- **Source**: post-plan asymptote checklist P-7
- **Canon location**: `agents-playbook.md` §7.9 Implementation-Readiness Audit (codified 2026-05-18; Wave 1 supervisor flagged as AMBIGUOUS — operational pattern with no named canon step; canonized this session)
- **Memory**: `memory://feedback_implementation_readiness_audit`
- **Phase-7 note**: §7.9 codifies 12-dimension audit + BLOCKER/STUCK/SLOW severity scale + distinct-from-SCRUM rationale + composition with /PLAN cycle Step 5.2

### #14 — Design-Choices vs Research-Grounded disclosure appendix (Part XIX.A)
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d COMPLETED — section authored)
- **Source**: post-plan asymptote checklist P-8
- **Canon location**: `architects-blueprint.md` §19.A Design Choices vs Research-Grounded Claims appendix (codified 2026-05-18; Wave 1 supervisor flagged Part XIX.A as MISSING; canonized this session) + Canon XXXV citation discipline
- **Memory**: `memory://feedback_design_choices_disclosure_appendix`
- **Phase-7 note**: prior "RATIFIED 2026-05-17" claim was Step-(d) failure (same drift class as #11); Phase 7 closed gap by authoring §19.A with 5-status enum + mandatory fields + closing disclosure box

---

## 2026-05-18 ironclaw-spine session — pending candidates

### #15 — agents-playbook §15.3.13 Wave Dispatch Protocol
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d complete via LÆX supervisor RATIFY-UPHOLD)
- **Source**: ironclaw-spine iter-11 follow-on (wave fan-out/fan-in mechanics)
- **Canon location**: `agents-playbook.md` §15.3.13 (cited at line 2344 footer)
- **Authority**: operator-authorized Canon XV override (2026-05-18)
- **Cross-canon ties**: composes with LASDLC v2.5.2 wave schema; Cookbook §66 context assembly; /BUILD skill v2 Step 11.3

### #16 — agents-playbook §7.8 Pre-Completion Verification Gate
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d complete via LÆX supervisor RATIFY-UPHOLD)
- **Source**: ironclaw-spine iter-11 (⚡PRE-DONE marker operationalization)
- **Canon location**: `agents-playbook.md` §7.8
- **Authority**: operator-authorized Canon XV override
- **Memory**: `memory://feedback_pre_completion_during_plan_authoring`

### #17 — agents-playbook §Phase-2A.5 Canon-Doc Amendment Phase Protocol
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d complete via LÆX supervisor RATIFY-UPHOLD)
- **Source**: ironclaw-spine iter-11 (canon-violation-by-construction prevention)
- **Canon location**: `agents-playbook.md` §Phase-2A.5
- **Authority**: operator-authorized Canon XV override
- **Memory**: `memory://feedback_canon_violation_by_construction`

### #18 — Pre-Done verification protocol (3-check: staleness + artifact-exists + canon-drift)
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d complete via LÆX supervisor RATIFY-UPHOLD)
- **Source**: ironclaw-spine iter-11 pre-completion fold
- **Canon location**: `agents-playbook.md` §7.8 (companion clause to #16)
- **Note**: distinct candidate because it codifies the VERIFICATION semantics; #16 codifies the gate
- **Memory**: `memory://feedback_pre_completion_during_plan_authoring`

### #19 — git_branching_invariants composite (4-doc fold)
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d complete via LÆX supervisor RATIFY-UPHOLD)
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
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d complete via LÆX supervisor RATIFY-UPHOLD)
- **Source**: ironclaw-spine iter-17 (operator audit: "Are tracking artifacts still valid?")
- **Composite of 3 sub-changes**:
  1. Per-build `manifest.yaml` `runtime_state` block
  2. NEW `gate_receipts.ndjson` append-only artifact (hash-chained per §SG-CRYPTO.3)
  3. `active.yaml` 3-field extension (`execution_mode`, `run_id`, `overlaps_with`)
- **Canon location**: `LASDLC-TEMPLATE-v1.yaml` v2.5.4 + `LASDLC-TEMPLATE-v1.CHANGELOG.md` v2.5.4 entry
- **Authority**: operator-authorized Canon XV override (2026-05-18)

### #21 — Canon XLII: Schema-Changelog Separation Doctrine
- **Status**: RATIFIED at Phase 7 2026-05-18 (Step d complete via LÆX supervisor RATIFY-UPHOLD)
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
- **Status**: RATIFY-AND-CODIFY at Phase 7 2026-05-18 (LÆX supervisor verdict; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine session 2026-05-18 (changelog-vs-canon-repo bundling)
- **Proposed canon home**: `builders-cookbook` new §N OR `agents-playbook` §Phase-1 (planning discipline)
- **Cross-canon ties**: Canon XV (operator authority), agents-playbook Phase-1, Cookbook §66 Context Assembly
- **Memory**: `memory://feedback_two_problems_one_question`
- **Self-author flag**: authored this session by Claude → independent verification at Phase 7 mandatory

### #23 — Per-build manifest 10-field counter sync discipline
- **Status**: RATIFY-AND-CODIFY at Phase 7 2026-05-18 (LÆX supervisor verdict; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine session 2026-05-18 (iter-18 counter sync after Canon XLII)
- **Proposed canon home**: `LASDLC-TEMPLATE-v1.yaml` §manifest-hygiene OR `agents-playbook` §counter-sync
- **Cross-canon ties**: architects-blueprint Part XXI, Cookbook §66 counter-derivation
- **Memory**: `memory://feedback_per_build_manifest_counter_sync`
- **Self-author flag**: authored this session → ceiling applies

### #24 — LÆX queue authoritative enumeration is Canon XXXIX Step-(b) prerequisite
- **Status**: RATIFY-AND-CODIFY at Phase 7 2026-05-18 (LÆX supervisor verdict; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine session 2026-05-18 (queue gap discovery)
- **Proposed canon home**: `platform-canon` Canon XXXIX extension OR `builders-cookbook` §audit-discipline
- **Cross-canon ties**: Canon XXXIX, Canon XLII, LASDLC manifest counters
- **Memory**: `memory://feedback_laex_queue_enumeration_prerequisite`
- **Self-author flag**: authored this session → ceiling applies; THIS queue file is the empirical witness

### #25 — Parallel-agent helix entry pre-write check
- **Status**: DEFERRED at Phase 7 2026-05-18 (LÆX supervisor DEFER verdict — N=1 evidence insufficient per Canon XXXIII; re-nominate after N≥2 independent parallel-dispatch builds confirm pattern frequency)
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine iter-18 (discovered parallel-agent enrichment)
- **Proposed canon home**: `agents-playbook` §7.2 (pre-tool discipline) OR `operators-manual` §helix-write-surface
- **Cross-canon ties**: teammateMode auto, operators-manual, security-guardrails
- **Memory**: `memory://feedback_parallel_agent_helix_check`
- **Self-author flag**: authored this session → ceiling applies

### #26 — Circular validation signature: canon authored from plan → re-XEA proves consistency
- **Status**: RATIFY-AND-CODIFY at Phase 7 2026-05-18 (LÆX supervisor verdict — N=2 iters confirm; pressure-tested iter-8 + iter-9; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: HIGH
- **Source**: ironclaw-spine 2026-05-18 iter-8 (Δ +1.3 canon-codification-driven)
- **Proposed canon home**: `architects-blueprint` Part XIV §14.3 (scoring honesty)
- **Cross-canon ties**: Canon XXXVI (quality-first compression), canon-audit-as-review-tier (Tier 3)
- **Memory**: `memory://feedback_circular_validation_canon_plan`

### #27 — TaskStop + relaunch tighter scope on stalled background agent
- **Status**: DEFERRED at Phase 7 2026-05-18 (LÆX supervisor DEFER — N=1 pressure-test below threshold; re-nominate at Phase 8 if another background-agent stall surfaces)
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine 2026-05-18 Task #17 stall (relaunched in 77s with tighter scope)
- **Proposed canon home**: `agents-playbook` §worker-lifecycle
- **Cross-canon ties**: operators-manual task lifecycle, Cookbook §agent-scoping
- **Memory**: `memory://feedback_taskstop_relaunch_tighter_scope`

### #28 — SCRUM 3-round verdict-upgrade signature (R3 upgrades = convergence proof)
- **Status**: RATIFY-AND-CODIFY at Phase 7 2026-05-18 (LÆX supervisor verdict; co-ratified with #30 as composite "SCRUM Round Convergence Signatures" section; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine 2026-05-18 R3 (SERAPH/AYIN/QUANTUM upgrades)
- **Proposed canon home**: `platform-canon` Canon XXXIII corollary OR `architects-blueprint` Part X
- **Cross-canon ties**: Canon XXXIII (independent verification), agents-playbook SCRUM protocol
- **Memory**: `memory://feedback_scrum_r3_verdict_upgrade_signature`

### #29 — TeammateMode + parallel-agent layer distinction
- **Status**: REJECTED at Phase 7 2026-05-18 (LÆX supervisor REJECT — duplicate of CLAUDE.md OPS-8.2 already operational; recommendation: enhance CLAUDE.md OPS-8.2 with memory entry's additional detail on tmux/AYIN visibility + context-inheritance asymmetry rather than canon promotion)
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine session (CLAUDE.md OPS-8.2 codification)
- **Proposed canon home**: `agents-playbook` §worker-dispatch OR `operators-manual` §agent-roles
- **Cross-canon ties**: OPS-8.2 layer selection rule, AgentRunner vs Claude Code teammate distinction
- **Memory**: `memory://feedback_teammate_mode_synergy`

### #30 — SCRUM Round-2 depth-on-new-surface signature
- **Status**: RATIFY-AND-CODIFY at Phase 7 2026-05-18 (LÆX supervisor verdict; co-ratified with #28 as composite "SCRUM Round Convergence Signatures" section providing N=2 evidence base; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: MEDIUM
- **Source**: ironclaw-spine R2 (gaps surfaced on R1-fold additions)
- **Proposed canon home**: `architects-blueprint` Part X (review convergence) — sibling to #28
- **Cross-canon ties**: Canon XXXIII, agents-playbook SCRUM rounds
- **Memory**: `memory://feedback_scrum_round_2_depth_signature`

### #31 — Self-review ceiling STRONG for LARGE+canon plans
- **Status**: REJECTED at Phase 7 2026-05-18 (LÆX supervisor REJECT — duplicate of Canon XXXIII + Architects Blueprint Part XIV §C2b already canonical; recommendation: add architecture-intelligence-substrate observation [87.6→92.9 delta] to Canon XXXIII helix entry as N=2 calibration evidence rather than new canon)
- **Confidence (sweep)**: MEDIUM
- **Source**: novel-substrate plan-validation observations across sessions
- **Proposed canon home**: `architects-blueprint` Part XIV §C2 (independent-runner gate)
- **Cross-canon ties**: Canon XXXIII (self-validation ceiling), agents-playbook review discipline
- **Memory**: `memory://feedback_self_review_ceiling_novel_substrate`

### #32 — Enum-extension collision pre-check
- **Status**: RATIFY-AND-CODIFY at Phase 7 2026-05-18 (LÆX supervisor verdict; new section §61 Enum-Extension Collision Pre-check; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: LOW (borderline — could stay operational)
- **Source**: webshell routes.ts session (constrained-vocabulary mutation)
- **Proposed canon home**: `builders-cookbook` §validation-discipline (lightweight pattern)
- **Cross-canon ties**: architects-blueprint Part VI (file-function map consistency)
- **Memory**: `memory://feedback_enum_collision_precheck`

### #33 — Comprehensive E2E console-error-zero requirement
- **Status**: RATIFY-WITH-MERGE at Phase 7 2026-05-18 (LÆX supervisor verdict; merge into Cookbook §57.6 as §57.6c Console-Error Zero Gate; canon body queued in LAEX-PHASE-7-CODIFICATIONS-2026-05-18.md; pending close-out application)
- **Confidence (sweep)**: LOW (overlap risk with existing Cookbook §57)
- **Source**: 2026-05-13 era Playwright discipline session
- **Proposed canon home**: `builders-cookbook` §57 extension (E2E Test Engineering Standards) — possible merge rather than separate section
- **Cross-canon ties**: Canon XXXII (E2E discipline), Northstar §S (pillar validation)
- **Memory**: `memory://feedback_comprehensive_e2e`
- **Overlap note**: existing §57 already covers E2E test engineering; Phase 7 should decide MERGE-INTO-§57 vs SEPARATE-SUBSECTION

---

---

## 2026-05-19 portfolio-pillar-drift session additions (Step (b) per Canon XXXIX)

Per `/REFLECT specifically for canon promotions` 2026-05-19. 10 candidates surfaced from one session arc covering: `/PLAN copilot-omniscience-read` (iter-1 → iter-2 cross-exam) → `/SYNC` skill creation → `/SCRUM portfolio-pillar-drift` → `/research active.yaml citation audit` → `/ENRICH` helix entry. Empirical evidence: 5 active.yaml citation patches applied; 36% MIS-CITED drift rate measured; 3-sibling SCRUM unanimous on pattern P-1 (Citation Integrity Doctrine).

### #34 — Cookbook §69: Citation Integrity Doctrine (TIER 1) — **RATIFIED 2026-05-19**
- **Status**: RATIFIED at Phase 7 2026-05-19 via Path D (LÆX hypothesis → XEA cross-exam → operator stamp). Canon edit applied to `builders-cookbook.md` §69. Cookbook bumped to v3.3.0.
- **Source**: /SCRUM portfolio-pillar-drift 2026-05-19 (R1 unanimous — LÆX RATIFY-AND-CODIFY + SOUL "necessary but insufficient → also Cookbook rule" + CORSO APPROVE-WITH-FIXES "needs §"); QUANTUM citation audit (36% MIS-CITED)
- **Proposed canon location**: `builders-cookbook.md` **§69** (next available; was §75 speculative — XEA placement correction)
- **XEA conditions applied** (cross-exam 2026-05-19):
  1. Placement corrected §75 → §69 (LÆX X-5)
  2. Add cross-references to Canon XXXV (parent doctrine) + Canon XLII (compatible discipline)
  3. **BLOCKING addition (XEA X-1)**: Tier-2 migration trigger — when inline audit comments accumulate to ≥3 in any single tracking-artifact file, migrate to companion `<file>.CHANGELOG.md` per Canon XLII Tier-2 trigger. Inline comments are valid only at Canon XLII Tier 0 (≤1 entry) or Tier 1 (≤2 entries with migration plan). Without this condition, #34 sets up a future Canon XLII violation as accumulated steady-state.
- **Self-application note**: active.yaml ALREADY at Canon XLII Tier-2 trigger after 5 inline audit comments applied 2026-05-19. Same-session migration to `active.yaml.CHANGELOG.md` recommended as housekeeping regardless of #34's ratification path.
- **Rule**: Tracking artifacts (active.yaml, portfolio.md, _MOC-builds.md, builds-registry.yaml) and plan frontmatter citing canonical concepts (Pillars, mechanical checks, Component Northstars, Canon sections) MUST use **verbatim canon strings** OR explicitly flag deviation as `*(paraphrased "<short>")*`. Forbidden: heuristic paraphrase without flag, composite citations without enumeration, implicit Pillar reference by number without verifying current canon definition. Required: verbatim heading, OR paraphrase-with-flag, OR inline audit-correction comment.
- **Authority pending**: LÆX Phase 7 ratification + operator stamp
- **Empirical witnesses**: 5 MIS-CITED builds patched same-session (gitforest-live-ops, replicated-greeting-robin, architecture-intelligence-substrate, squishy-dancing-thimble-phase-a, harvesting-converging-quasar); P4 column in portfolio.md sparse → dense after correction; copilot-omniscience-read iter-2 B-1 self-correction (canary)
- **Convergent evidence (Canon XXXIX item 1)**: 3 of 3 SCRUM siblings (LÆX, SOUL, CORSO) independently surfaced rule; QUANTUM audit produced 36% drift rate
- **Biblical grounding (item 2)**: Proverbs 18:13 ("answering before listening is folly") — applies to citing canon without reading canon
- **Decision-shaping (item 3)**: changes every plan-authoring + tracking-artifact-edit decision; enforces /XEA Layer 2 mechanical-check verification
- **Pressure-tested (item 4)**: 2026-05-19 audit corrected 5 builds; /SYNC dry-run drift detector caught D-1 before /BUILD
- **Cross-doc impact**: Cookbook §75 (primary) + Operators-Manual §"Tracking artifact conventions" (cross-ref) + Architects Blueprint Part XIX (cross-ref) + LASDLC schema (validation hook)
- **Memory**: `feedback_citation_integrity_doctrine` (to be authored)
- **Helix entry**: `helix://shared/entries/2026-05-19-portfolio-pillar-drift-scrum.md` (Pattern P-1)

### #35 — Canon XXXIX Extension: Operator-Grade vs Canon-Grade Vocabulary Divergence Doctrine (TIER 1, PROVISIONAL)
- **Status**: PROVISIONAL_QUEUED — XEA-cleared 2026-05-19; canon edit DEFERRED pending 2nd empirical witness (per operator decision Path D). Operator chose: "let next session produce the second example" rather than ratify on N=1. Re-evaluate at next Phase-7 with 2nd witness.
- **Source**: /SCRUM portfolio-pillar-drift 2026-05-19 R1 — LÆX explicit: *"canon-vs-operator-intent collision is real and load-bearing... If operator intent has drifted from canon prose, the gap deserves a LÆX promotion candidate, not silent overwrite."*
- **Proposed canon location**: `platform-canon.md` **Canon XXXIX Extension subsection** (NOT new Canon XLIII, NOT Canon XV amendment — XEA-confirmed placement). Precedent: Canon XXXIX already carries one extension subsection ("Authoritative Enumeration as Step-(b) Prerequisite", ratified 2026-05-18 #24).
- **XEA corrections applied** (cross-exam 2026-05-19):
  1. Placement decided: Canon XXXIX subsection (operator-stamped under Canon XV authority; matrix-protocol formally waived — see procedural-gap note below)
  2. **Witness count corrected (XEA X-2)**: was `1.5` → now `1` (self-citation of LÆX's own R1 opinion excluded per Canon XXXIII Self-Validation Ceiling)
  3. **Biblical grounding swapped (XEA X-3)**: primary was Matthew 22:21 (defensible but stretched); now Proverbs 22:1 ("a good name is rather to be chosen than great riches") — direct fit for operator-semantic-stake in working vocabulary. Matthew 22:21 retained as secondary (authority-hierarchy context).
  4. **Procedural gap addressed (XEA X-4)**: #35 touches ≥2 canon docs (Canon XV + Canon XXXIX + Canon I cross-refs) + had 3 contested placement options — normally triggers LDB Matrix Ratification (LÆX Section D). **Matrix protocol formally waived under Canon XV operator authority** for this candidate. Rationale: LÆX placement reasoning was sound (precedent exists with #24 subsection), evidence is N=1 PROVISIONAL anyway (matrix vote on weak evidence inflates confidence falsely), and the operator-grade-vs-canon-grade question is itself an operator-authority topic. Operator may reverse this waiver and run matrix protocol if 2nd empirical witness arrives and changes calculus.
- **Cross-references added** (XEA X-4): Canon XV (authority hierarchy preserved), Canon I (canon supremacy default), Canon XXXIII (self-validation ceiling justifies witness-count correction)
- **PROVISIONAL framing**: explicit "PROVISIONAL — single empirical witness 2026-05-19; second pressure-test pending in different domain" annotation required on canon edit. Re-evaluate at next Phase-7 with 2nd witness.
- **Rule**: When operator working vocabulary diverges from canon prose for the same concept (e.g. "Vibe coding orchestration" vs canon "Secure-by-Default Agent Orchestration" for Pillar 2), the divergence is an LÆX promotion candidate, NOT a silent overwrite. Decision tree: (1) canon wins by default (Canon I), mechanical citation corrected; (2) flag divergence as Canon XXXIX promotion candidate; (3) never silently overwrite operator's vocabulary in personal artifacts (CLAUDE.md, draft plans, memory) without explicit decision.
- **Authority pending**: LÆX Phase 7 ratification + operator stamp
- **Empirical witnesses**: 2026-05-19 portfolio.md drift (P2 "Vibe coding orchestration" vs canon "Secure-by-Default Agent Orchestration") — LÆX correctly raised this as canon-vs-operator-intent question rather than silent overwriting
- **Convergent evidence (item 1)**: only LÆX flagged in R1; SOUL + CORSO converged on mechanical fix but didn't surface the divergence question — partial convergence; may need second pressure-test before ratification
- **Biblical grounding (item 2)**: Proverbs 22:1 ("a good name is rather to be chosen than great riches") — operators have semantic stake in their working vocabulary
- **Decision-shaping (item 3)**: changes every canon-vs-operator-vocabulary tension resolution; prevents silent canon overwrites of operator artifacts
- **Pressure-tested (item 4)**: 1 session — limited; recommend ratification gated on second pressure-test in different domain
- **Cross-doc impact**: Canon XV (Principal Hierarchy) + Canon XXXIX (promotion pipeline) — explicit interaction needs LÆX layer mapping
- **Memory**: `feedback_operator_grade_vs_canon_grade_divergence` (to be authored)
- **Helix entry**: `helix://shared/entries/2026-05-19-portfolio-pillar-drift-scrum.md` (Lesson Learned #6)

### #36 — CLAUDE.md BLOCKING POLICIES: Pre-/BUILD drift check (TIER 2 — CLAUDE.md, not canon) — **APPLIED 2026-05-19**
- **Status**: APPLIED to `~/.claude/CLAUDE.md` BLOCKING POLICIES section 2026-05-19. Operator-stamped per Canon XV (CLAUDE.md is personal global instructions).
- **Source**: This session — /SYNC dry-run drift detection caught 4 portfolio mislabels before /BUILD copilot-omniscience-read; if not caught, /BUILD would have spawned a worktree with citation drift propagating downstream
- **Proposed location**: `~/.claude/CLAUDE.md` BLOCKING POLICIES section
- **Rule**: Before invoking `/BUILD <codename>`, run `/SYNC --roadmap <codename> --dry-run` to detect tracking-artifact drift (D-1 through D-6 findings). If HIGH-severity drift detected, resolve first (separate /SCRUM or operator decision) before /BUILD creates worktree.
- **Authority**: operator stamp on CLAUDE.md (no LÆX needed for personal global instructions)
- **Pressure-tested (item 4)**: 2026-05-19 — D-1 detector caught 4 mislabels; /BUILD held; correction applied; /BUILD now safe to proceed
- **Memory**: `feedback_pre_build_drift_check` (to be authored)

### #37 — CLAUDE.md COMMUNICATION COVENANT rule 11: Audit-pending disclosure (TIER 2) — **APPLIED 2026-05-19**
- **Status**: APPLIED to `~/.claude/CLAUDE.md` COMMUNICATION COVENANT as rule 11 (2026-05-19). Operator-stamped per Canon XV.
- **Source**: This session — portfolio.md "Active builds" column re-population done in 2 phases (P0 SCRUM flagged `_audit pending_`, P1 audit populated). Disclosure-at-checkpoint preserved operator trust.
- **Proposed location**: `~/.claude/CLAUDE.md` COMMUNICATION COVENANT (add rule 11)
- **Rule**: When a field's correctness depends on a deferred audit task, flag the field with `_audit pending_` (or structured equivalent) and a link to the dispatched task. Truthful-by-disclosure beats truthful-by-omission.
- **Authority**: operator stamp on CLAUDE.md
- **Pressure-tested**: 2026-05-19 portfolio.md re-population
- **Memory**: `feedback_audit_pending_disclosure` (to be authored)

### #38 — Mid-SCRUM verification supersedes formal R2 (TIER 3 — memory only)
- **Status**: WRITTEN to memory 2026-05-19 (Path D — not canon-level)
- **Rule**: When SCRUM R1 surfaces a claim with verifiable evidence (line numbers, file paths), moderator should verify mid-SCRUM before R2 dispatch. If verification produces stronger findings than peer critique would, skip R2 → R3.
- **Pressure-tested**: 2026-05-19 — SOUL's L54 claim verified mid-SCRUM expanded scope from 4 portfolio labels to 4 labels + 2 canon prose corrections; R2 would have added little
- **Memory**: `feedback_mid_scrum_verification_supersedes_r2`

### #39 — Inline tracking-artifact audit comments (Canon XLII-compatible) (TIER 3)
- **Status**: WRITTEN to memory 2026-05-19 (Path D)
- **Rule**: When correcting tracking-artifact citations (active.yaml, portfolio.md), append inline `# YYYY-MM-DD audit (severity): was X — Y rationale` comment. One-line scope preserves provenance without violating Canon XLII (no amendment history in schema files).
- **Pressure-tested**: 2026-05-19 — 5 active.yaml patches all carried inline provenance comments; git log preserves full change record
- **Memory**: `feedback_tracking_artifact_inline_audit_comments`

### #40 — Pre-Northstar-v1.1 vocabulary fingerprints (TIER 3)
- **Status**: WRITTEN to memory 2026-05-19 (Path D)
- **Rule**: Strings like `"both P1+P2"`, `both_p1_p2`, `"both"` in `pillar_mapping:` indicate plans authored under pre-v1.1 (2-Pillar) Northstar canon. Audit-via-grep is effective: `grep -rE 'pillar_mapping.*"both"' ~/.claude/plans/` flags candidates for re-citation.
- **Pressure-tested**: 2026-05-19 — squishy-dancing-thimble-phase-a + eef-e5 + harvesting-converging-quasar all used "both" vocabulary; flagged for audit
- **Memory**: `feedback_pre_v1_1_vocabulary_fingerprints`

### #41 — Iter-2 cross-exam self-correction as canary for systemic drift (TIER 3)
- **Status**: WRITTEN to memory 2026-05-19 (Path D)
- **Rule**: When a plan's iter-2 cross-exam catches a citation error IN ITSELF (e.g. copilot-omniscience-read B-1: P4→P6), grep all sibling artifacts for the same drift pattern. One canary catch typically reveals systemic drift.
- **Pressure-tested**: 2026-05-19 — copilot-omniscience-read iter-2 B-1 was the canary; grep revealed 4 other MIS-CITED builds with same P4→P6 drift
- **Memory**: `feedback_iter2_canary_self_correction`

### #42 — Drift asymmetry threshold → /SYNC --lint priority (TIER 3)
- **Status**: WRITTEN to memory 2026-05-19 (Path D)
- **Rule**: 36% MIS-CITED drift rate (5 of 14 in-flight builds) is high enough to mandate `/SYNC --lint` as priority backlog (not nice-to-have). Below 10% drift = lint nice-to-have; 10-25% = should-have; ≥25% = priority.
- **Pressure-tested**: 2026-05-19 — 36% empirical rate from QUANTUM audit
- **Memory**: `feedback_drift_asymmetry_lint_threshold`

### #43 — Canon docs can drift against themselves (TIER 3)
- **Status**: WRITTEN to memory 2026-05-19 (Path D)
- **Rule**: Canon documents can be internally contradictory (prose vs table, intro vs body). Verify surrounding context, not just cited line. SOUL's L54 evidence chain was correct on the existence of drift but incomplete on the scope (missed L58-64 table aligning with canon).
- **Pressure-tested**: 2026-05-19 — Operators-Manual §1.2 L54 prose "four Pillars (P1–P4)" contradicted its own table L58-64 (all 7 Pillars listed)
- **Memory**: `feedback_canon_internal_contradiction`

---

## Aggregate

| Status | Count | Sources |
|---|---|---|
| RATIFIED | 21 (#4–#14, #34, #47–#52, #54–#56) | 2026-05-17 (11) · 2026-05-19 (#34) · 2026-05-23 (#47–#52, #54–#55) · 2026-05-30 (#56) |
| PENDING — Step b (memory + candidate identified, contradiction check pending) | 15 (#1–#3 + #22–#33) | 2026-05-15 + 2026-05-18 memory-sweep additions |
| PENDING — Step c+ (Canon XV override applied; LÆX ratification pending) | 7 (#15–#21) | 2026-05-18 ironclaw-spine in-session canon edits |
| DEFERRED | 1 (#53) | N=2 trigger pending |
| PENDING — Step d (contradiction PASS; Kevin's stamp required) | 0 | — |
| **Total candidates** | **44** | 33 pre-sweep + #34 + #47–#56 |
| **Ratification target** | ≥11 at phase boundary | met; coverage now 44% (18/41) |
| **Currently ratified** | 18/41 | phase boundary met; next batch promotion target ≥25 (Step-b promotions #15–#21 + #22–#33) |

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

---

## Wave 2026-05-22 — ENG-KHADAS-AUDIT-20260522 /REFLECT

Source: ENG-KHADAS-AUDIT-20260522 security fix session + Cloudflare Zero Trust Access deployment.
Pressure-tested: all 3 candidates empirically validated in the same session.

Note: SG-1 through SG-4 (Security Guardrails amendments) were applied directly under Canon XV
operator-override (Kevin's stamp, 2026-05-22). They are `canon_location_prelisted` and do NOT
need Phase 7 ratification — the changes are already in the canon body. GATE skill candidates
(#44–#46) require a plugin PR against `light-architects-plugins` + LÆX co-sign.

### #44 — GATE skill Q2: --lib --bins exception for pre-existing test failures (TIER 2)

| Field | Value |
|-------|-------|
| **Status** | PENDING |
| **Target doc** | GATE skill Q gate section (plugin: `lightarchitects/1.0.0/skills/GATE`) |
| **Source** | ENG-KHADAS-AUDIT-20260522 — Q2 `--all-targets` blocked on pre-existing `arch_cache` gap |
| **Memory** | `memory://feedback_gate_clippy_scope` |
| **Contradiction check** | PENDING — against GATE skill Q2 command + CLAUDE.md quality gates |
| **Evidence** | `--all-targets` blocked gate on 6 test files not in `git diff github/main --name-only`; `--lib --bins` produced 0 warnings; documented in `.gate-evals/phase-3-merge.yaml` |

**Amendment text**:
> After the Q2 `cargo clippy --workspace --all-targets --all-features -- -D warnings` command, add: *Exception: if failures appear in files NOT listed by `git diff <base>..HEAD --name-only` (pre-existing structural failures), scope to `cargo clippy --lib --bins -- -D warnings` and document each failing file under `pre_existing_issues_documented` in the gate eval YAML.*

---

### #45 — GATE skill V0: RustEmbed dist/ symlink prerequisite for lightarchitects-sdk worktrees (TIER 2)

| Field | Value |
|-------|-------|
| **Status** | PENDING |
| **Target doc** | GATE skill V0 worktree prerequisite section |
| **Source** | ENG-KHADAS-AUDIT-20260522 — pre-commit hook proc-macro panic on missing dist/ |
| **Memory** | `memory://feedback_rustembedist_gate_worktree` |
| **Contradiction check** | PENDING — against GATE skill existing "frontend build if *-ui/ touched" note |
| **Evidence** | Gate worktree creation without dist/ symlink caused RustEmbed proc-macro panic; symlink from primary resolved it; failure reproduced, fix confirmed |

**Amendment text**:
> Extend the V0 worktree frontend build note: *For workspaces containing `#[derive(RustEmbed)]` crates (check: `grep -r "RustEmbed" */Cargo.toml`), symlink the UI dist/ directory from primary into the gate worktree before any cargo invocation, regardless of whether the diff touches the UI. Failure mode is a proc-macro panic at compile time, not a missing-file error. Command: `ln -s <primary>/<crate>-ui/dist <worktree>/<crate>-ui/dist`.*

---

### #46 — GATE skill S1: MEDIUM in-gate fix criteria (TIER 2)

| Field | Value |
|-------|-------|
| **Status** | PENDING |
| **Target doc** | GATE skill S1 security section |
| **Source** | ENG-KHADAS-AUDIT-20260522 — S1-R1 MEDIUM `Zeroizing` fix applied in-gate |
| **Memory** | `memory://feedback_in_gate_security_fix` |
| **Contradiction check** | PENDING — GATE skill S1 currently only specifies HIGH/CRITICAL escalation; MEDIUM path unspecified |
| **Evidence** | `[0u8; 4]` → `Zeroizing::new([0u8; 4])` met all 3 criteria; gate passed without Kevin escalation; YAML recorded `status: RESOLVED`; test suite confirmed |

**Amendment text**:
> After the HIGH/CRITICAL escalation rule, add: *MEDIUM findings may be resolved in-gate (without blocking the gate or escalating) when all three hold: (a) fix is on the same branch; (b) fix strengthens or completes an existing change rather than adding new logic; (c) change is ≤5 lines with zero ambiguity in correctness. Record in gate eval YAML as `status: RESOLVED` with commit SHA and description. If any criterion fails, surface to operator before continuing.*

---

---

### #47 — Canon N1B: Linux systemd Credential Delivery via `systemd-creds` + `LoadCredential=` (TIER 1)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED |
| **Ratified by** | Kevin (Canon XV operator stamp) · LÆX |
| **Ratified date** | 2026-05-22 |
| **Target doc** | `security-guardrails.md` §5.5.1 (new subsection) |
| **Source** | ENG-KHADAS-AUDIT-20260522 — F-3 HIGH: `EnvironmentFile=` pattern in Khadas sibling units violates §5.5 "no secrets in env vars in production" (CWE-214, T1552.001) |
| **Contradiction check** | PASS — §5.5 bullet "no secrets in env vars" is the governing rule; §5.5.1 specifies the mechanism to satisfy it on Linux; no conflict with macOS Keychain pattern |
| **Evidence** | `EnvironmentFile=-/home/khadas/.arena.env` in la-soul + la-eva; `EnvironmentFile=-/home/khadas/.seraph/scope.toml.env` in la-seraph; `LoadCredential=` stubs already present in all 3 units pre-ratification |

**Amendment text**: added as §5.5.1 in `security-guardrails.md` — specifies `systemd-creds encrypt` provisioning, `LoadCredential=` unit syntax, `$CREDENTIALS_DIRECTORY` binary read pattern, [S] HIGH gate rule for EnvironmentFile credential delivery, and 5-step migration sequence.

**Operational follow-up** (Kevin, Khadas box — separate from ratification):
1. Provision secrets: `sudo systemd-creds encrypt --name=arena-pepper --tpm2-device=auto - /etc/credstore/la-arena-pepper`
2. Equivalent for NEO4J_PASSWORD, EVA session key, SERAPH scope secret
3. Activate stubs: uncomment `LoadCredential=` lines; remove `EnvironmentFile=` (secrets) from units
4. Update sibling binaries to read from `$CREDENTIALS_DIRECTORY/` and rebuild + deploy
5. `systemctl --user daemon-reload && systemctl --user restart la-soul la-eva la-seraph`

---

---

### #48 — Pre-/BUILD Empirical Spike Trigger (TIER 1 doctrine)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan — `architects-blueprint.md` §4.12 authored |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `architects-blueprint.md` Part IV (Research-First Doctrine) — new §4.12 OR Part VIII (Project Planning Framework) |
| **Source** | khadas-npu-soul-embeddings Phase 2 spike (2026-05-22) — empirically caught RKNN INT8 catastrophic failure + bge-large regression BEFORE /BUILD committed |
| **Contradiction check** | PASS — extends Canon XXXII (research-first) with empirical arm; no conflict with existing §4.x research doctrine; consistent with Canon XLI Diagram-First (same "intervene before commit" pattern at different lifecycle stage) |
| **Witness count** | **N=2** — (a) khadas-npu-soul-embeddings spike 2026-05-22, (b) khadas-neo4j-foundation plan derived from spike outcome. MEETS Canon XXXIX threshold for planning-doctrine candidates. |
| **Memory** | `memory://feedback-pre-build-empirical-spike` (write at canon application time) |

**Amendment text** (LÆX draft, ready to apply):

> **§4.12 (or §8.4) Empirical Spike Trigger (Pre-/BUILD)**
>
> When any SM-N tier-1 condition depends on a measurement with no published prior art (Context7 returns no library ID; arXiv/HF hub search returns 0 directly-applicable papers; no internal helix witness ≥N=1 with matching hardware/runtime profile), the plan MUST schedule a **standalone empirical spike** in a separate phase (or separate pre-build plan) **before** /BUILD invocation. The spike is a single-purpose measurement: run the exact pipeline against the exact substrate; record p50/p95/accuracy/cosine/etc. as an artifact committed to the build's `_research/` directory. If the spike outcome contradicts the SM-N target threshold, the plan REWRITES (different SM-N, different substrate, different stack) *before* /BUILD — not at gate-2 fallback. Pressure-tested 2026-05-22: khadas-npu-soul-embeddings spike measured RKNN encoder mean cosine 0.56 vs 0.95 SM-1 target → plan held → foundation build (khadas-neo4j-foundation) inserted upstream; saved ~5–9hr fallback arc + surfaced 6 HIGH integration issues unrelated to embeddings.

**LASDLC-TEMPLATE-v1.yaml addition**: new optional `phase_0.empirical_spike` block.

---

### #49 — HNSW Vector Index Dimension Lock (TIER 2 gotcha)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan — `builders-cookbook.md` §72 authored |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `builders-cookbook.md` Neo4j-related section (new bullet under hardening/operations) |
| **Source** | khadas-neo4j-foundation iter-1 SOUL R1 (2026-05-22) — caught default NomicEmbedTextV15 (768) vs deployed v10 index (384) mismatch |
| **Contradiction check** | PASS — Cookbook has no existing HNSW-dim guidance; security-guardrails §3.6 Neo4j hardening covers auth/binding only; cross-link both. |
| **Witness count** | N=1 with mechanical fix (single Cypher assertion) — Cookbook gotchas with concrete fix promote at N=1. |
| **Memory** | (none yet — fold into Cookbook on apply) |

**Amendment text** (LÆX draft):

> **HNSW dimension lock** — Neo4j HNSW vector indexes have their dimension fixed at `CREATE INDEX` time. Any code path that assumes the embedding-provider config dimension matches the deployed index dimension MUST verify against the running database before use: `SHOW INDEXES WHERE name = '<index-name>' YIELD options` and assert `options.indexConfig['vector.dimensions']` equals the configured model's output dim. Dimension drift (e.g., default-config NomicEmbedTextV15 768-dim against an index built at 384-dim under migration v10) does NOT raise a clear error at query time — it returns spurious results or silent truncation. Add a startup-time check in any HelixStore-consuming binary; fail-fast with a canonical message naming both the configured dim and the index dim. Pressure-tested 2026-05-22 (khadas-neo4j-foundation iter-1 caught the default-NomicEmbedTextV15-768d vs deployed-384d mismatch before /BUILD).

**Cross-canon**: security-guardrails §3.6 Neo4j hardening (cross-link); could become a [Q] gate item for any helix-touching build.

---

### #50 — Neo4j Community Docker Image: GDS Plugin NOT Bundled (TIER 2 deploy gotcha)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan — `operators-manual.md` §Neo4j-Docker-Deploy authored |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `operators-manual.md` Neo4j deployment section + `builders-cookbook.md` cross-link |
| **Source** | khadas-neo4j-foundation iter-1 SOUL R1 (2026-05-22) — caught missing GDS plugin would silently fail Node2Vec writes |
| **Contradiction check** | PASS — operators-manual has no Neo4j-Docker section currently; this CREATES the canonical entry. |
| **Witness count** | N=1 with verbatim fix command — operational deploy gotchas promote at N=1 when fix is concrete. |

**Amendment text** (LÆX draft):

> **Neo4j community Docker image — GDS plugin not bundled.** The official `neo4j:5.21.2-community` image ships without the Graph Data Science (GDS) plugin. HelixDb migration v10 creates an HNSW index that depends on GDS at write-time. First write to the helix silently fails (or returns confusing "procedure not found" errors) unless the container is launched with `NEO4J_PLUGINS='["graph-data-science"]'` AND `NEO4J_dbms_security_procedures_unrestricted=gds.*`. Smoke test: `docker exec <container> cypher-shell -u neo4j -p <pwd> 'CALL gds.version()'` — must return a version string, not an error. Add to every Neo4j Docker deploy script.

---

### #51 — Probe-Before-Assert for Unfamiliar Systems (TIER 1 truthfulness control)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan — `platform-canon.md` Canon XLIV authored |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `agents-playbook.md` Communication Covenant operationalization section OR `platform-canon.md` (new Canon candidate) |
| **Source** | 2026-05-22 Khadas optimal-architecture analysis — initial assessment contained 5+ fabricated claims (drive reformattable, NPU not loaded, 2.5GbE link speed, etc.) corrected only by direct hardware probes |
| **Contradiction check** | PASS — extends Communication Covenant rule 8 (honest uncertainty, KNOW vs DON'T-KNOW vs ASSUMING) with operational mechanism; reinforces Canon XXXIII self-validation ceiling (independent probe = cross-verification); aligns with LÆX #37 audit-pending disclosure (truthful-by-disclosure family) and agents-playbook §15.4.5 post-commit tree verification. |
| **Witness count** | **N=1 with 5+ fabricated claims caught**. Communication-Covenant operational extensions threshold: N≥1 with mechanical evidence chain. **MEETS THRESHOLD**. |

**Amendment text** (LÆX draft):

> **Probe-before-assert for unfamiliar systems.** When reasoning about hardware, deployed-system state, vendor toolchains, or any substrate not present in the assistant's training data with high coverage, an explicit mechanical probe (shell command output, `lsblk`, `lspci`, `ip link`, `dpkg -l`, container manifest inspect, vendor `--version`) MUST precede any assertion of fact about that system. Pattern-match from name (e.g., "Edge2 has 2.5GbE") is forbidden — it routinely fabricates plausible-sounding wrong claims. Pressure-tested 2026-05-22 khadas-neo4j-foundation prep: initial Khadas analysis included ≥5 fabricated claims (drive reformattable, NPU loaded, 2.5GbE link speed, etc.) — all corrected only after `ip link`, `lsblk`, `lsmod | grep rknpu`, `ethtool` probes ran. The Communication Covenant rule 8 *principle* (label KNOW vs ASSUMING) becomes operationally enforceable here: the **probe output is the KNOW**; without it, the claim is ASSUMING and must be labeled as such.

---

### #52 — Retrieval Baseline Must Exercise Target Retrieval Mode (TIER 2 benchmark-design rule)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan — `builders-cookbook.md` §73 authored |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `builders-cookbook.md` (new §retrieval-benchmarking) + `architects-blueprint.md` Part V (Shipped-Means) cross-link |
| **Source** | khadas-neo4j-foundation iter-1 EVA + SOUL R1 concurrent finding (2026-05-22) — empty-helix p50 measurement triggers KeywordDominated mode (65/25/3/7 BM25), biasing baseline away from vector-path target |
| **Contradiction check** | PASS — Cookbook has no benchmark-design entry; Blueprint Part V (Shipped-Means / SM-N tier-1 conditions) gains a cross-link. |
| **Witness count** | N=1 (khadas-neo4j-foundation SM-5 / Phase 5.5). Concurrent SCRUM finding from 2 siblings increases confidence. Promote as Cookbook benchmark-design rule. |

**Amendment text** (LÆX draft):

> **Retrieval baseline must exercise the target retrieval mode.** SOUL's adaptive retrieval (4-signal RRF) picks weights based on corpus size: ≤24 steps → `KeywordDominated` (65/25/3/7); 25–99 → `Balanced` (25/35/30/10); ≥100 → `VectorDominated` (etc.). Any `soul.helix.retrieve` baseline measurement intended for apples-to-apples comparison against a later embedding-pipeline change MUST first seed the helix to the same retrieval-mode tier the post-change measurement will hit — otherwise the baseline measures BM25 latency, not vector-index latency. Concrete pattern: before measuring p50, ingest N canonical documents such that `25 ≤ step_count ≤ 99` (Balanced) or `≥100` (VectorDominated), assert the mode in test setup, then measure. Pressure-tested 2026-05-22 khadas-neo4j-foundation Phase 5.5: 8 canon docs seeded to land Balanced mode for SM-5 baseline.

**Cross-canon**: Blueprint Part V; SOUL crate docs (4-signal RRF doc); EVA observability (AYIN trace must record retrieval-mode-selected per query for verification).

---

### #53 — Pre-/BUILD Integration-Surface Inspection (DEFERRED — N=2 trigger)

| Field | Value |
|-------|-------|
| **Status** | DEFERRED pending N=2 witness |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `agents-playbook.md` §15.6 (new section) |
| **Source** | khadas-neo4j-foundation pre-/BUILD inspection (2026-05-22) — caught 6 HIGH integration findings |
| **Witness count** | N=1 substantial. Canon XXXIX strict threshold N≥2 for new agents-playbook sections. |
| **Promotion trigger** | Next /BUILD against a running system that catches ≥1 HIGH finding via identical `ss -tnp` / `docker inspect` / `SHOW INDEXES` probing methodology. Memory-only entry NOW with explicit promotion-trigger annotation: `memory://feedback-pre-build-integration-inspection`. |
| **Distinguished from LÆX #36** | /SYNC compares plan-to-tracking-artifacts; §15.6 (candidate) compares plan-to-running-system. Different problem class. |

---

### #54 — Store Fields from External Service Boundaries Are Untrusted at Runtime (TIER 2 Svelte/TS gotcha)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan — `builders-cookbook.md` §74 authored |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `builders-cookbook.md` §74 (new) + `CLAUDE.md` Svelte non-negotiables summary line |
| **Source** | webshell-drag-drop implementation session (2026-05-23) — `task.sibling.toUpperCase()` crashed Dashboard reactive graph; `task.sibling` declared `string` in TypeScript but `undefined` at runtime for in-flight conductor task entries |
| **Contradiction check** | PASS — §51.1 (boundary sanitization) covers adversarial injection, not operational non-null contract failures. No other canon entry addresses TypeScript non-null guarantees at external service store boundaries. |
| **Witness count** | N=1. Pattern generalises broadly across all stores fed by conductor/SSE/MCP. TIER 2 gotcha threshold met. |

---

### #55 — Defer Drag-Source State in `ondragstart` via `requestAnimationFrame` (TIER 2 browser API gotcha)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan — `builders-cookbook.md` §75 authored |
| **Evaluated by** | LÆX (2026-05-23) |
| **Target doc** | `builders-cookbook.md` §75 (new) + `CLAUDE.md` Svelte non-negotiables summary line |
| **Source** | webshell-drag-drop implementation session (2026-05-23) — synchronous `draggingPanelId.set(panelId)` in `ondragstart` caused browser to snapshot dimmed drag ghost; fix: `requestAnimationFrame(() => draggingPanelId.set(panelId))` |
| **Contradiction check** | PASS — no existing canon covers HTML5 DragEvent ghost snapshot timing. §74 and §75 are sibling Svelte runtime rules from the same session. |
| **Witness count** | N=1. Behaviour is deterministic browser spec (ghost captured synchronously during dragstart handler). TIER 2 gotcha threshold met. |

---

### #56 — Frontend HITL Surface Implementation Contract (PENDING Kevin's stamp)

| Field | Value |
|-------|-------|
| **Status** | RATIFIED 2026-05-30, Canon XV, Kevin Francis Tan — `agents-playbook.md §7.5.1` authored |
| **Evaluated by** | LÆX (2026-05-30) |
| **Target doc** | `agents-playbook.md` §7.5.1 (new subsection after §7.5 HITL checkpoint flow) |
| **Source** | cockpit-wave-composer × ironclaw-autonomous-e2e cross-examination (2026-05-30) — CRITICAL plan gap found: `pendingEscalations` in `Cockpit.svelte` is display-only (no action buttons); plan assumed it was a valid HITL resolution surface; amendment required to add inline ironclaw resolution panel. Memory entry: `memory://reference-webshell-three-hitl-systems`. |
| **Contradiction check** | PASS — §7.5 covers build-gate HITL checkpoint flow (gate review: Accept/Reject/Modify/Override). §7.5.1 is additive: covers frontend *component implementation* requirement for agent action HITL surfaces. No existing canon entry addresses the action-path requirement or prohibits display-only HITL surfaces. Security Guardrails §3.3 covers nonce (AES-256-GCM unique nonce per message) but not HITL-specific replay prevention. Builders Cookbook §6.1 covers cost HITL checkpoints, not agent action surfaces. No conflicts. |
| **Witness count** | N=1 (this session). Principle is first-principles derivable: HITL by definition requires a human decision loop; display-only rendering breaks the loop. Nonce requirement follows from Security Guardrails §3.3 applied to HITL endpoints. LÆX assessment: TIER 2 subsection threshold (not a new top-level section; extends existing §7.5). |
| **Distinguished from existing canon** | §7.5 = build-gate review flow. §7.5.1 = frontend implementation contract for agent escalation surfaces. Different problem class. |

**Proposed §7.5.1 text (exact — ready to apply on Kevin's stamp):**

```markdown
### 7.5.1 Frontend HITL Surface Implementation Contract

Every webshell HITL surface that displays an agent escalation **must ship with a resolution
action path**. Display-only rendering — showing escalation data with no Approve/Reject action —
is not a valid HITL implementation. It provides false assurance to operators while the
underlying agent remains blocked indefinitely.

**Three requirements (all mandatory):**

1. **Action path required** — every HITL card renders at minimum an Approve and Reject action.
   The resolution call must be a POST to a named backend endpoint, not a fire-and-forget event.

2. **Typed subscription for new events** — legacy event pipelines that feed display-only arrays
   (e.g., `la:escalation` window event → `pendingEscalations` in Cockpit.svelte) must not be
   repurposed for new agent action flows. New agent HITL events use a dedicated typed SSE
   subscription via `subscribeByTopic()`.

3. **Nonce-validated resolution** — resolution endpoints for agent actions must validate a
   server-minted escalation nonce (UUIDv7) to prevent replay attacks. The nonce is embedded in
   the resolution request body and verified server-side against a replay-kill set. The nonce must
   never appear in logs or error messages (Security Guardrails §3.3, CWE-209).

**Violation class**: a HITL surface that renders agent escalation data but carries no resolution
action path is a [S] gate violation, routed to SERAPH.
```

---

*See also*: `canon://platform-canon` Canon XXXIX (the pipeline) · `canon://platform-canon` Canon XLII (separation doctrine) · `helix://corso/builds/ironclaw-spine/manifest.yaml` (counters)
