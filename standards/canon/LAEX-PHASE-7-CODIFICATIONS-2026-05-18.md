# LÆX Phase 7 Codifications — 2026-05-18

**Status**: Holding file for proposed canon body texts from RATIFY-AND-CODIFY verdicts. Applied to canon docs at Phase-7 close-out (task #44).
**Constitutional basis**: Canon XXXIX Step (d) — LÆX ratification + Kevin's stamp produces canon-body text.

---

## #22 — Two-Problems-in-One-Question Framing Discipline

**Verdict**: RATIFY-AND-CODIFY (Wave 4)
**Canon home**: `builders-cookbook.md` new §N OR `agents-playbook.md` §Phase-1
**Section title**: §N — Two-Problems-in-One-Question Framing Discipline

```markdown
## §N — Two-Problems-in-One-Question Framing Discipline

**Problem**: Operator requests that bundle two solutions in one sentence ("we need X or maybe Y") often hide two distinct concerns with different urgency and cost profiles. Conflating them produces over-engineered solutions or wrong-problem recommendations.

**Pattern** — the framing test:

When you see "do X or do Y" language:

1. **Restate as two separate questions**: "X solves what?" + "Y solves what?"
2. **Check problem distinctness**: do X and Y solve different problems or different aspects of the same problem?
3. **Match problems to symptoms**: which problem matches the immediate trigger for the request?
4. **Tier independently**:
   - Problem 1: urgent cost/benefit analysis → Tier 1 (execute now) or conditional
   - Problem 2: conditional cost/benefit analysis → Tier 2 (defer until trigger condition) or orthogonal
5. **Present trade-offs per-problem**, not per-solution

**Why this catches real gaps**: Bundled phrasing masks diagnostic uncertainty. The honest read is often "I noticed something wrong and have multiple hypotheses." Treating it as "pick X or Y" produces recommendations that solve one problem confidently while ignoring the other, or solve a compound problem when only one was urgent.

**Pressure-tested**: 2026-05-18. Operator request: "move the changelog somewhere else **or** create a repo for canon files." Framing test surfaced two problems: (1) Schema-bloat (LASDLC at 7567 lines, v2.5.4 about to be 4th tail block) — solved by extracting to CHANGELOG.md companion (10 min); (2) Canon access-control / publication-readiness (canon in sdk repo; sdk may go public) — multi-day effort, no immediate trigger. Applied Tier 1 in 5 commits (30 min). Tier 2 deferred with explicit trigger conditions. The "OR" framing made these look like alternatives; they aren't.

**Composition**: Canon XV (operator authority over problem diagnosis), Architects Blueprint Part I (thesis clarity), Builders Cookbook §66 (context assembly), agents-playbook §7.9 (implementation-readiness audit).

**Related anti-pattern**: solving the wrong problem confidently. This discipline is the gate that catches it before commitment.
```

---

## #23 — Manifest Counter Synchronization (10-Field Discipline)

**Verdict**: RATIFY-AND-CODIFY (Wave 4)
**Canon home**: `architects-blueprint.md` Part XXI extension (manifest governance) OR `LASDLC-TEMPLATE-v1.yaml` §manifest-hygiene

```markdown
## Manifest Counter Synchronization (10-Field Discipline)

**Scope**: ironclaw-style per-build `manifest.yaml` carries counter and reference fields that all derive from canon state. When canon changes (new amendment, ratification, version bump), ALL of the following must update synchronously — partial updates create drift between canon and manifest that breaks downstream queries.

**Why**: The manifest is a SNAPSHOT — a frozen reflection of canon state at build-start. Partial snapshots claim N amendments while pointing to N+1 docs, or claim 20 candidates while the queue enumerates 21. /BUILD G6 preflight reads manifest counters and verifies consistency; mismatches HALT dispatch.

**10-field atomic checklist** — when you edit canon:

1. `canon_amendments_applied` (int) — cumulative count across the build's session arc
2. `canon_docs_touched` (int) — distinct files modified (separate from amendments)
3. `lasdlc_v` (string) — current LASDLC schema version; bump only on schema change
4. `blueprint_parts_extended` / `cookbook_sections_added` / `agents_playbook_sections_added` (lists) — per-doc section enumeration; append new section IDs
5. `lex_promotion_candidates` (int) — current count of candidates in queue
6. `lex_ratification_target` (string with embedded count) — "≥N/M ratified at phase boundary"; update BOTH ratio and target counts
7. `lex_pre_authored_candidates` (int) — sub-pieces of composite candidates
8. `dependent_canon` (list) — per-canon-doc descriptors; append to correct doc
9. `metadata.version` (string) — bump on every sync (1.2 → 1.3 → 1.4 ...)
10. `metadata.last_updated` (string) — ISO date + iter-N stamp + brief change description

**Pattern**: list all 10 fields at start of sync, update in one batch (4–5 sequential edits since they're in different manifest sections), grep-verify before commit.

**Anti-pattern**: updating counter N but missing counter N+1. Silent drift. Catches at next /BUILD dispatch → gate HALT.

**Pressure-tested**: 2026-05-18 iter-18. Canon XLII codification required updating 9 of 10 fields. Missing the `lex_ratification_target` string bump would have left manifest claiming a ratio that didn't match the enumerated queue.

**Composition**: Part XXI (manifest governance) + Canon XLII (manifest is CHANGELOG-class artifact).
```

---

## #24 — Canon XXXIX Extension: Authoritative Enumeration as Step-(b) Prerequisite

**Verdict**: RATIFY-AND-CODIFY (Wave 4)
**Canon home**: `platform-canon.md` Canon XXXIX extension

```markdown
## Canon XXXIX Extension — Authoritative Enumeration as Step-(b) Prerequisite

**Scope**: Canon XXXIX describes a four-step pipeline (create entry → identify candidates → contradiction check → ratification). Step (b) "identify promotion candidates" is incomplete if the candidates are not enumerated in a single source of truth.

**Problem**: Numbered candidate IDs scattered across canon doc footers + per-build manifest counters do NOT constitute a queue. They are pointers to a queue that doesn't exist. Without authoritative enumeration:

- Phase N LÆX cannot mechanically compute "for each candidate, run contradiction check"
- Kevin's ratification stamp has no canonical anchor
- Counter claims drift (manifest says 20, only 7 IDs found in docs)
- Ratified candidates from prior sessions become archaeologically invisible

**Pattern**: Before LÆX Phase-N evaluation fires:

1. **Author `standards/canon/LAEX-PHASE-N-QUEUE.md`** — single source of truth enumerating ALL candidates
2. **Per-candidate required fields**: ID + title · status (`RATIFIED | PENDING-Step-b | PENDING-Step-c+`) · source · canon location · authority · memory/helix cross-references · cross-canon ties
3. **For composite candidates**: enumerate sub-pieces
4. **Link manifest to queue**: per-build manifest carries `lex_queue_file` field; manifest counters DERIVE from queue
5. **Update on every canon edit**: when authoring under Canon XV override, add to queue FIRST, then make canon edit

**Anti-pattern**: Queue drift — manifest claims N candidates, queue enumerates N+1 or N-1. Discovered when LÆX begins step (c) contradiction check and finds undocumented candidates.

**Pressure-tested**: 2026-05-18 iter-18. Manifest claimed `lex_promotion_candidates: 20` but searching all 10 canon docs found only ~7 numbered IDs. Authored LAEX-PHASE-7-QUEUE.md reconstructing full 21-candidate enumeration. Discovered 11 candidates already RATIFIED from prior sessions that would have been silently dropped without the queue.

**Composition**: Canon XXXIX (the pipeline) + Canon XLII (queue is CHANGELOG-class) + Canon XXXIII (independent runner clears self-validation ceiling on queue enumeration).

**Mandatory trigger**: Every /BUILD reaching phase-boundary evaluation must have completed this queue before LÆX Phase-N evaluation fires.
```

---

## #26 — Circular Validation Signature (Architects Blueprint §14.3)

**Verdict**: RATIFY-AND-CODIFY (Wave 4)
**Canon home**: `architects-blueprint.md` Part XIV §14.3 amendment

```markdown
### §14.3 — Circular Validation Signature: Canon-Codification-Driven Score Lift

**Pattern**: When canon docs are authored FROM a plan's patterns (mid-session canon-fold during phase-2A or phase-2A.5) and the plan is then re-XEA'd against the updated canon, score Δ in the re-XEA is **canon-codification-driven**, not plan-improvement.

**Why this matters**: The naive interpretation of "iter Δ +1.3" is "plan got better." That's wrong here. The plan didn't change; the rubric caught up with what the plan was already saying. Different signal entirely.

This tests the canon's coherence: if the canon was correctly amended FROM the plan's patterns, the plan should score higher against the new rubric than the old rubric (same plan body). If it doesn't, something in the canon-amendment process drifted from what the plan actually says.

**When to expect this**:

After a session that:
1. Authors a substantial plan defining new patterns
2. Folds those patterns into canon docs (via Phase 2A.5-class amendment)
3. Re-runs /XEA against the updated canon

The first re-XEA produces a circular-validation lift. Subsequent re-XEAs (with canon stable) revert to normal iter-improvement deltas or stop-rule convergence.

**Honest reporting** (Score-honesty discipline, §14.2):

When publishing the verdict:
- Cite the rubric source change explicitly in `xea_verdict.amendment_citations`
- Note in `ceiling_annotation` that self-iter plan-body delta = 0
- Skill stop-rule still fires when subsequent iters return Δ < 0.3 (canon-codification is a one-time lift)
- Convergence is between plan + canon (not just self-iter)

**Pressure-tested**: 2026-05-18 ironclaw-spine iter-8 (Δ +1.3, plan-body-unchanged, 21 canon amendments applied; circular validation proof). iter-9 drift-fold (Δ +0.1, normal). iter-10 wave-decomposition substantive add (Δ +0.55, plan content changed). Pattern confirmed.

**Composition**: Extends §14.2 (score honesty) + Canon XXXVI (quality-first compression) + Canon XXXIII (self-validation ceiling — circular validation is post-amendment verification).
```

---

## #28 + #30 — SCRUM Round Convergence Signatures (composite)

**Verdict**: RATIFY-AND-CODIFY composite (Wave 5)
**Canon home**: `architects-blueprint.md` Part X (Review Convergence) OR new Canon XXXIII corollary
**Section title**: SCRUM Round Convergence Signatures — R2 depth + R3 upgrade as fold-verification

```markdown
### SCRUM Round Convergence Signatures (R2 depth + R3 upgrades = fold-verification)

SCRUM rounds carry diagnostic signatures beyond their per-lens verdicts. Two complementary patterns characterize honest convergence — both must be present, or the cycle isn't done.

#### R2 — depth-on-new-surface signature

R2's expected trajectory is **depth refinement on R1's just-added fold**, not breadth on R1's pre-existing surfaces. When R2 produces ~50–60% fewer findings than R1, the pattern is honest: R1 caught broad issues; iter-2 folds addressed them; R2 now finds finer concerns specifically on what iter-2 added.

If R2 produces breadth on old surfaces (≥80% of R1 finding count, distributed across pre-existing scope), iter-2 folds were inadequate — they didn't land on what R1 surfaced. Iteration-3 fold required; R2 cannot serve as convergence proof.

#### R3 — verdict-upgrade signature

R3 is typically interpreted as a consensus check. Its real diagnostic value is the **upgrade signature** — proof that R2 folds actually addressed R2 critics' findings.

| Upgrades / 7 siblings | Reading |
|---|---|
| 0 + new BLOCKING surfaced | R2 folds didn't address findings; iter-N+1 required |
| 1–2 | Folds partially landed; targeted re-iteration recommended |
| 3 ± 1 | Folds substantially landed; convergence is REAL; SCRUM cycle complete |
| 5+ | Suspicious — check for groupthink or insufficient adversarial rigor |

#### Composite reading

A complete SCRUM cycle exhibits BOTH:
- R2 depth-on-new-surface (~50–60% fewer findings than R1, focused on iter-2-fold zone)
- R3 verdict-upgrades (3 ± 1 of 7 siblings)

Either signature alone is insufficient: depth without upgrades = folds touched the surface but didn't satisfy critics; upgrades without depth = siblings updated verdicts without re-inspecting the new fold.

#### Pressure-tested

- **R3 upgrade signature**: 2026-05-18 ironclaw-spine. R2 downgrades 3/7 (SERAPH, QUANTUM, EVA). iter-4 XL + Phase 2A restructure. R3 upgrades 3/7 (SERAPH HOLD→SHIP, AYIN GAPS→READY, QUANTUM RED-adjacent→CLEAR). Real convergence.
- **R2 depth signature**: 2026-05-17 architecture-intelligence-substrate. R1 findings → iter-2 folds → R2 produced 57–67% fewer findings concentrated on iter-2 additions.

#### Composition with Canon XXXIII

Canon XXXIII establishes that same-author verification catches ~70% of defects. SCRUM (7 independent lenses) operates orthogonally — catches the remaining ~30% across each lens's domain. The R2+R3 convergence signatures prove that the 30% actually got fixed at code/spec level, not just acknowledged at verdict level.
```

---

## #25, #27, #29, #31 — DEFER or REJECT (no codification)

| # | Verdict | Reason | Recommendation |
|---|---------|--------|----------------|
| #25 (parallel-agent helix check) | DEFER | N=1 evidence | Re-nominate after N≥2 parallel-dispatch builds |
| #27 (TaskStop relaunch) | DEFER | N=1 evidence | Re-nominate at Phase 8 if another background-agent stall surfaces |
| #29 (teammateMode synergy) | REJECT | Duplicate of CLAUDE.md OPS-8.2 | Upgrade CLAUDE.md OPS-8.2 with memory entry's additional detail (tmux vs AYIN visibility; context-inheritance asymmetry) |
| #31 (self-review ceiling) | REJECT | Duplicate of Canon XXXIII + Blueprint §C2b | Add architecture-intelligence-substrate observation (87.6→92.9 delta) to Canon XXXIII helix entry as N=2 calibration point |

---

## #25 — DEFER (parallel-agent helix entry pre-write check)

**Verdict**: DEFER (Wave 4 — Canon XXXIII evidence insufficiency)
**Reason**: N=1 pressure-test (iter-18 discovery of parallel-agent helix entry). Pattern is operationally sound but evidence base is thin for canon ratification.
**Recommendation**: track as operational guidance in memory + `agents-playbook §worker-lifecycle` notes (not formal canon yet). Re-nominate after N≥2 independent parallel-dispatch builds confirm the frequency and impact.
**Status**: candidate remains as memory entry; no canon body change at Phase 7 2026-05-18.

---

## #32 — Cookbook §61 Enum-Extension Collision Precheck (NEW)

**Verdict**: RATIFY-AND-CODIFY (Wave 6)
**Canon home**: `builders-cookbook.md` new §61

```markdown
## §61 Enum-Extension Collision Pre-check

**Principle**: Before any plan claims a new value in an existing enum (e.g., "BuildDetail view-mode-6 = Wave Timeline"), pre-check the canon to verify the position is free and the position count aligns.

**Rule S61a — Pre-claim enumeration audit**: For every enum position claimed in a plan body:
1. Grep canon (e.g., `webshell-api-surface-v1.md` §3.3) for the enum's current count + all variants
2. Verify the claimed position is the next free slot (not a collision with existing variant)
3. If collision discovered: propose different position OR declare rename with operator approval
4. Plan body assertion: cite source enum file + line range (not just "view-mode-N")

**Rule S61b — Common enums requiring pre-check**:
- `BuildViewMode` (`webshell-api-surface-v1.md` §3.3)
- `WebEvent` variants (events/types.rs + canon §1.3)
- `AgentDomain` (`lightarchitects/src/soul/types.rs`)
- Gate vocab `[A+S+Q+C+O+P+K+D+T+R]` (Canon XXXVIII)
- LASDLC tier `SMALL | MEDIUM | LARGE`
- Status enums per surface

**Rule S61c — Cross-plan coordination**: When two plans both extend the same enum, the helix coordination pact MUST declare who claims which value. First merger commits the canonical extension; second extends additively.

**Pressure-tested**: `gitforest-live-ops` iter-7 API-canon audit caught view-mode-6 collision (pre-existing `comms` occupied that position). Resolved iter-8 by allocating Wave Timeline as view-mode-7 + helix coordination pact declared enum-scoped allocation.
```

---

## #33 — Cookbook §57.6c Console-Error Zero Gate (MERGE into §57.6)

**Verdict**: RATIFY-WITH-MERGE (Wave 6)
**Canon home**: `builders-cookbook.md` §57.6 (Stability Tiers and CI Gates) — add sub-rule §57.6c

```markdown
### §57.6c Console-Error Zero Gate (ADDITION)

**Rule S57.6c**: All E2E test runs (Smoke, Capability, Integration tiers) must terminate with zero console errors logged during test execution. Console error captures (from §57.2a `console.ndjson`) are reviewed; any TypeError, uncaught exception, or unhandled promise rejection must be diagnosed and fixed before test passes.

**Clarification**: §57.2b requires console errors to be captured (artifact discipline); §57.6c requires console errors to be ZERO (blocking gate). Capturing five errors and ignoring them is non-compliant. The evidence-bundle must contain `consoleLogs: []` OR explicitly document that captured errors are benign (intentional deprecation warnings, caught exceptions in error-recovery tests) via allowlist.

**Implementation**: Playwright `page.on('console')` accumulates all messages; test teardown asserts `errors.length === 0` OR `errors.filter(e => !ALLOWLIST.includes(e.text)).length === 0`.

**Pressure-tested**: Webshell-ui test suite discipline (2026-05, Playwright session). TypeError capture during comprehensive E2E catches hydration mismatches, event-handler closures, and stale promise rejections that would ship silently in production.
```

---

## REJECTED with operational follow-up (no canon write, but action items)

| # | Verdict | Operational follow-up action |
|---|---|---|
| #29 (teammateMode synergy) | REJECTED | Enhance CLAUDE.md OPS-8.2 with memory entry's additional detail (tmux/AYIN visibility, context-inheritance asymmetry) — operational doc update at close-out |
| #31 (self-review ceiling) | REJECTED | Update Canon XXXIII helix entry adding architecture-intelligence-substrate observation (87.6→92.9) as N=2 calibration evidence |

---

## Application order at Phase-7 close-out (task #44)

1. Apply #24 first (Canon XXXIX extension — meta-canon governs all other ratifications)
2. Apply #22 (two-problems framing — independent decision discipline)
3. Apply #23 (manifest counter sync — operational discipline)
4. Apply #26 (circular validation signature — extends existing §14.x)
5. #25 stays deferred (no canon write)

Each application: 1 Edit per canon doc + queue status update.
