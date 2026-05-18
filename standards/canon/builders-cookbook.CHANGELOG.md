# Builders Cookbook — Amendment History

Companion changelog for `builders-cookbook.md`. The cookbook holds **current state only**; this file holds the **amendment narrative** — section added, source build, canon reference, rationale — that `git log` doesn't capture in narrative form.

**Authoritative latest version**: see the header inline summary in `builders-cookbook.md`.
**Mechanical history**: `git log -- standards/canon/builders-cookbook.md`

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
