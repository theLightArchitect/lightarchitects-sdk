# Agents Playbook — Amendment History

Companion changelog for `agents-playbook.md`. Schema doc holds **current state only**; this file holds the **amendment narrative**.

**Authoritative version**: see footer stamp in `agents-playbook.md`.
**Mechanical history**: `git log -- standards/canon/agents-playbook.md`
**Constitutional basis**: Canon XLII — Schema-Changelog Separation. Created 2026-05-18 at Phase 7 close-out when section count crossed Tier-2 threshold.

---

## v1.9 — Phase 7 ratifications (2026-05-18, iter-19)

**Source**: LÆX Phase 7 ratification walkthrough (see `LAEX-PHASE-7-QUEUE.md`).

### §7.9 — Implementation-Readiness Audit
**Candidate**: #13 — promoted from `feedback_implementation_readiness_audit`
**Wave**: 1 (Step-(d)-failure closure — memory marked RATIFIED 2026-05-17 but canon body was AMBIGUOUS — operational pattern with no named canon step)
**Rationale**: Distinct review class post-SCRUM, pre-/BUILD. Catches concreteness gaps no quality/security/perf/a11y review surfaces — undeclared deps, missing schemas, hand-waved invocations, undeclared paths, missing API URLs. 12-dimension audit + BLOCKER/STUCK/SLOW severity scale. SCRUM R3 converges to ~93 STRONG; cannot reach EXEMPLARY without independent impl-readiness audit pushing ceiling +0.5 to +1.0.
**Cross-canon ties**: §7.7 (worker completion gate), §15.4.5 (phantom-commit prevention), Canon XXXIII (independent-runner principle)
**Pressure-tested**: `gitforest-live-ops` iter-7 audit surfaced 12 BLOCKERS + 11 STUCK + 8 SLOW that 6 prior iterations missed; iter-8 fix pass closed all 12 BLOCKERS (23/23 RESOLVED).

### §7.10 — Two-Problems-in-One-Question Framing Discipline
**Candidate**: #22 — promoted from `feedback_two_problems_one_question`
**Wave**: 4 RATIFY-AND-CODIFY (HIGH conf, supervisor verdict)
**Rationale**: Operator requests bundling two solutions ("do X or do Y") often hide two distinct concerns with different urgency/cost profiles. Pre-implementation triage discipline: restate as two questions → check distinctness → match to symptoms → tier independently → present trade-offs per-problem. Prevents over-engineered solutions or wrong-problem recommendations.
**Cross-canon ties**: Canon XV (operator authority over problem diagnosis), Architects Blueprint Part I (thesis clarity), Cookbook §66 (Context Assembly Discipline), §7.9 (Implementation-Readiness Audit)
**Pressure-tested**: 2026-05-18 ironclaw-spine iter-18 — "move changelog OR create canon repo" bundling split surgically into Tier 1 (schema-bloat, 10 min) + Tier 2 (canon access-control, deferred).
**Self-author note**: candidate authored this session by Claude; cleared Canon XXXIII self-validation ceiling via cold-context Explore-agent supervisor (Wave 4).

---

## v1.7–v1.8 — iter-11 + iter-15 amendments (2026-05-18)

**Sources**: ironclaw-spine iter-11 follow-on (§15.3.13 + §7.8 + §Phase-2A.5) + iter-15 git-awareness fold (§15.3.13.5 28-gate checklist).

### §15.3.13 + §15.3.13.5 — Wave Dispatch Protocol + Pre-Dispatch Checklist (24 gates)
**Candidates**: #15, #19 (composite git_branching_invariants)
**Wave**: 3 RATIFY-UPHOLD
**Rationale**: Operationalizes wave fan-out/fan-in mechanics + 24+4 hardcoded git-aware gates (PP×4 + PW×7 + PT×7 + PoT×3 + PoW×3 + cross-doc×4). Closes "git-aware throughout the build" operator concern.

### §7.8 — Pre-Completion Verification Gate
**Candidate**: #16 + #18 (verification protocol companion)
**Wave**: 3 RATIFY-UPHOLD
**Rationale**: ⚡PRE-DONE marker operationalization — 3-check verification (staleness ≤14 days + artifact-exists + canon-drift) before /BUILD treats marker as authoritative.

### §Phase-2A.5 — Canon-Doc Amendment Phase Protocol
**Candidate**: #17
**Wave**: 3 RATIFY-UPHOLD
**Rationale**: Closes canon-violation-by-construction antipattern. Inserts dedicated canon-amendment phase between research (Phase 2A) and implementation (Phase 3) so code-in-flight doesn't contradict declarative canon prose.

---

## v1.6 — iter-11 mid-session additions (2026-05-18)

### §HITL-7, §15.3a, §11.3a
**Wave**: 3 RATIFY-UPHOLD (operator-authorized Canon XV overrides upheld at Phase 7)
- §HITL-7 Escalation Notification Invariant
- §15.3a Tier Separation Principle (Supervisor / Worker / Git)
- §11.3a Canon-as-Cached-System-Prompt

---

## Earlier versions

See `git log -- standards/canon/agents-playbook.md`.

---

## Conventions for future amendments

1. Schema file = current state only per Canon XLII
2. Per-doc SemVer from v1.9 forward (Phase 7 baseline)
3. One CHANGELOG entry per version bump with sections / candidates / wave / supervisor verdict / cross-canon ties / pressure-test
4. LÆX candidate tracking until ratification
