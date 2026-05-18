# Platform Canon — Amendment History

Companion changelog for `platform-canon.md`. Schema doc holds **current state only**; this file holds the **amendment narrative** — Canon rationale, scriptural grounding, ratification provenance, cross-canon composition.

**Authoritative version**: see footer stamp in `platform-canon.md`.
**Mechanical history**: `git log -- standards/canon/platform-canon.md`
**Constitutional basis**: Canon XLII — Schema-Changelog Separation. Platform Canon is the home of XLII itself; self-application begins here at Phase 7 close-out (Tier-2 threshold reached via Canon XL + XLI + XLII + XXXIX extension).

---

## v2.3 — Canon XXXIX Extension (2026-05-18, Phase 7 iter-19)

### Canon XXXIX Extension — Authoritative Enumeration as Step-(b) Prerequisite
**Candidate**: #24 — promoted from `feedback_laex_queue_enumeration_prerequisite`
**Wave**: 4 RATIFY-AND-CODIFY (HIGH conf, supervisor verdict)
**Rationale**: Canon XXXIX describes a 4-step pipeline (entry → candidates → contradiction check → ratification). Step (b) "identify promotion candidates" is incomplete without authoritative enumeration in a single source-of-truth file. Numbered IDs scattered across canon doc footers + manifest counters do NOT constitute a queue — they're pointers to a queue that doesn't exist. Without enumeration: Phase N LÆX cannot mechanically compute "for each candidate, run contradiction check"; Kevin's stamp has no canonical anchor; counter claims drift; ratified candidates from prior sessions become archaeologically invisible.
**Mandatory trigger codified**: every /BUILD reaching phase-boundary evaluation MUST have completed LAEX-PHASE-N-QUEUE.md authoring before LÆX evaluation fires.
**Cross-canon ties**: Canon XXXIX (the pipeline this extends), Canon XLII (queue is CHANGELOG-class), Canon XXXIII (independent runner clears self-validation ceiling on enumeration authoring).
**Pressure-tested**: 2026-05-18 iter-18 — manifest claimed `lex_promotion_candidates: 20` but searching all 10 canon docs found only ~7 numbered IDs. Authored LAEX-PHASE-7-QUEUE.md reconstructing full 21-candidate enumeration; discovered 11 candidates already RATIFIED from prior sessions that would have been silently dropped. Subsequent 137-entry memory sweep surfaced 12 additional candidates (#22-#33) — queue prevented duplicative enumeration. Phase 7 walkthrough itself was load-bearing only because the queue existed.
**Self-author note**: candidate authored this session by Claude; cleared Canon XXXIII self-validation ceiling via cold-context Explore-agent supervisor (Wave 4). This canon is the empirical witness — the queue file is the artifact the canon describes.

---

## v2.2 — Canon XLII: Schema-Changelog Separation Doctrine (2026-05-18, iter-18)

### Canon XLII — Schema-Changelog Separation Doctrine
**Candidate**: #21 — promoted from `feedback_schema_changelog_separation_canon_xlii`
**Wave**: 3 RATIFY-UPHOLD (operator-authorized Canon XV override upheld by Phase 7 supervisor)
**Rationale**: A canonical standards document declares what is true NOW. The history of how it became true lives elsewhere. Three jobs (current state / amendment narrative / mechanical history) live in three places (schema doc / CHANGELOG.md companion / git log) and are never commingled. Closes the schema-as-its-own-changelog antipattern that produced LASDLC 7500-line tail-amendment accretion, cookbook 4-zone version-number conflicts, and security-guardrails orphan-row table drift.
**Mechanical predicate** (per Canon XLII):
```
schema_clean := no_tail_amendment_blocks
              ∧ no_scattered_version_entries
              ∧ pointer_to_changelog_present
```
**Tier-based migration triggers**: Tier 0 (single footer, defer) → Tier 1 (2 entries, plan migration) → Tier 2 (3+ entries, migrate immediately) → Tier 3 (multi-zone drift, reconciliation required).
**Empirical witnesses**: 3 CHANGELOG.md files committed in `b797ca3` (LASDLC) + `62edefa` (cookbook + security-guardrails) + this file series at Phase 7 close-out (architects-blueprint + agents-playbook + platform-canon).
**Cross-canon ties**: Canon XII (Living Standard — THAT canon evolves), Canon XXXIX (HOW evolution flows), Canon XLII (WHERE amendment narrative lives). Triple composition forms complete canon-maintenance discipline.
**Self-application note**: Canon XLII's home (platform-canon.md) is now migrating to companion CHANGELOG (this file) at Phase 7 close-out — the doctrine self-applies first. Tier-2 threshold reached via Canon XL + Canon XLI + Canon XLII + Canon XXXIX Extension within one session.
**Pressure-tested**: 2026-05-18 iter-18 operator concern surfaced antipattern; same-session migration revealed cookbook had Tier-3 multi-zone drift worse than the LASDLC trigger doc; Phase 7 walkthrough validated the doctrine binds itself.

---

## v2.1 — LDB §D5 + Gatekeeper Registry Extension + Vocabulary Canon (2026-05-18, iter-15+)

### LDB §D5 — Program Manifest Integrity Contract
**Wave**: 3 RATIFY-UPHOLD (part of #19 git_branching_invariants composite)
**Rationale**: Cryptographic ceremony for autonomous-mode build manifests — Ed25519 signature, HKDF subkey rotation, decisions.md hash-chain, manifest_id binding receipts. Composes with security-guardrails §SG-CRYPTO.1-.3.

### Gatekeeper Registry Extension — Decision Pipeline as runtime arbiter
Documents how the [A+S+Q+C+O+P+K+D+T+R] 10-gate registry composes with autonomous-mode decision-pipeline runtime mechanism.

### Vocabulary Canon — LightArchitect:* ↔ sibling mapping
Public surfaces use "agent" + "Squad"; internal surfaces preserve sibling identity. Vocabulary canon load-bearing per `memory://project_vocabulary_canon`.

---

## v2.0 — Canon XL: Mixture-of-Experts Platform Architecture (2026-05-14)

**Status**: ratified 2026-05-14 (Kevin direct, paired with Northstar Pillar 3 addition).
**Rationale**: Platform architecture formalized as MoE — multi-model, multi-agent routing with specialization and ensemble verification. Pairs with Northstar Pillar P3 mechanical checks.

---

## v1.9 — Canon XLI: Diagram-First Doctrine (2026-05-17)

**Status**: ratified 2026-05-17 (Kevin direct via Canon XXXIX pipeline; LÆX RATIFY WITH AMENDMENT cleared with scripture grounding confirmed).
**Rationale**: Architecture diagrams in LASDLC Phase 1 are design artifacts, not documentation outputs. Mechanical [A] gate predicate: `[A] passes := diagram_present ∧ drift_clean ∧ checklist_current`. Converts subjective gate to mechanical (three falsifiable conjuncts). Tier-based depth (SMALL=C3; MEDIUM=C2+C3; LARGE=C1+C2+C3+C4+ERD+sequence; PROGRAM=all+per-build subset). Source: `architecture-intelligence-substrate` SCRUM Round 1 LÆX critique.

---

## Earlier versions (Canon I–XXXIX)

See `git log -- standards/canon/platform-canon.md` for full history of Canons I–XXXIX. Notable recent:
- Canon XXXIX (2026-05-13): The Canon Promotion Pipeline (4 steps: entry → candidate → contradiction check → ratification)
- Canon XXXVIII (2026-05-05): Gatekeeper Expansion — [C] Canon + [R] Research+Risk gates
- Canon XXXVII (2026-05-05): Knowledge Gate Doctrine — [ASQPTDOK] vocabulary expansion
- Canon XXXVI (2026-05-04): Quality-First Compression Sequencing
- Canon XXXV (2026-05-04): Confidence Threshold Gate (≥95% required, ≥99.99% preferred)
- Canon XXXIV (2026-05-04): Confidence Interval Reporting
- Canon XXXIII (2026-05-04): Self-Validation Ceiling Doctrine (~70% same-author, independent runner clears remaining ~30%)

---

## Conventions for future amendments

1. **Schema file = current state only** per Canon XLII (this canon's own doctrine, applied to its home)
2. **Per-doc SemVer from v2.3 forward** (Phase 7 baseline)
3. **One CHANGELOG entry per version bump** with Canon ID / candidate / wave / scriptural grounding / cross-canon composition / pressure-test
4. **Canon entries require biblical grounding** per Canon Evaluation Criteria (or explicit N/A note for operational extensions)
5. **LÆX candidate tracking** until Phase-N ratification; then update status
