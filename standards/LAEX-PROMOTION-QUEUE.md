# LÆX Promotion Queue

> Canon XXXIX step (b) — promotion candidates pending ratification.
> Format: one candidate per block. Status transitions: pending → reviewing → ratified | rejected | superseded.

---

## Candidates

### Cookbook §N — Backend-gap UI overlay pattern (MockWrapper + isEnabled gate)

- **candidate_id:** LAEX-CAND-2026-05-20-001
- **source_build:** webshell-mock-overlay-shipping
- **predecessor_build:** webshell-unplugged-audit (abandoned with full review history preserved)
- **date_filed:** 2026-05-20
- **status:** pending-laex-ratification
- **filed_by:** Claude (Engineer)
- **cross_canon_ties:**
  - Cookbook (primary — proposed §N: backend-gap UI overlay pattern)
  - DESIGN-LANGUAGE.md (planned new section after webshell-event-bus-redesign Phase 5)
  - Northstar P4 (Operator-Legible Arc — direct alignment)
  - Canon XLII (Schema-Changelog Separation — pattern is UI schema, per-surface application is changelog)
- **helix_entry:** `~/lightarchitects/soul/helix/shared/entries/2026-05-20-mock-overlay-pattern.md`
- **proposed_canon_text:**
  > When a UI component depends on a backend endpoint that is not yet implemented
  > (or behind a `false` feature flag), render it with typed mock data inside a
  > `<MockWrapper>` (grayscale + corner badge) instead of leaving an empty/error
  > state. Operators see the intended UX with the gap visibly traceable to a
  > follow-up build codename. The pattern requires:
  > 1. Typed mock data in `mock-surfaces.ts` (getters compute fresh timestamps on read)
  > 2. Design-token-based MockBadge styling (no raw hex; use `--la-warn-mock-*` aliased to `--la-semantic-warn`)
  > 3. `role="note"` (not `role="status"`) — no SR live-region noise
  > 4. `inert` attribute on MockWrapper to suppress keyboard focus into mock subtree
  > 5. `[MOCK]` prefix on any decision-like or security-relevant string (prevents screenshot misinterpretation)
  > 6. MockBadge MUST NOT be nested inside heading elements (`<h1>`–`<h6>`); place as sibling inside `position: relative` parent
- **review_lens_history:** "3 review lenses converged through 4 iterations: XEA structural (81.55→85.30→88.40→88.85), frontend-design aesthetic, SCRUM 7-sibling × 2 rounds. SCRUM round 2 recommended STOP-ITERATING — convergence proven."
- **empirical_witness:** "Shipped via webshell-mock-overlay-shipping PR (4 surfaces: Intake autonomous, DecisionLog, WorktreePanel, ConductorPanel)"
- **contradiction_check:** "Verified 2026-05-20 — no contradiction with Northstar P4, Cookbook §code-standards, Security Guardrails §3.1, or Canon XLII"
- **expected_section:** Cookbook §N (next available — likely §69 if §68 is the latest)

---

### Cookbook §M — Structured-topic SSE supersedes pub/sub broker at ≤3-host scale

- **candidate_id:** LAEX-CAND-2026-05-20-002
- **source_build:** webshell-event-bus-redesign (LARGE, in-flight planning)
- **date_filed:** 2026-05-20 (filed pre-ship; full ratification deferred until that build's Phase 5 documentation)
- **status:** pending-research (Phase 0 of source build will ratify before promotion review)
- **filed_by:** Claude (Engineer), based on LÆX + frontend-design dual-lens consultation
- **cross_canon_ties:**
  - Cookbook (primary)
  - Northstar P2 (Vibe Coding Orchestration)
  - Agents Playbook §III (A2A envelopes — non-contradiction check required)
- **proposed_canon_text:**
  > For multi-consumer monitoring at ≤3-host scale, use structured-topic SSE on
  > a single in-process gateway hub. Cards subscribe by topic prefix via
  > `subscribeByTopic('topic.*.prefix', handler)`. The gateway IS the broker.
  > Introduce a pub/sub broker (NATS JetStream, Redis Streams, etc.) only when
  > ≥3 of the following 6 inflection conditions are met simultaneously:
  > (1) ≥3 independent consumer types
  > (2) cross-host messaging required
  > (3) independent producer/consumer deploy cadence
  > (4) topic-based filtering at scale (server-side match preferred over client filter)
  > (5) durable replay window required
  > (6) backpressure with queueing semantics
- **expected_section:** Cookbook §M (one after §N above)

---

## Process Notes

- Per Canon XXXIX, session lessons do NOT auto-apply. Pipeline:
  (a) memory entry → (b) promotion candidate (THIS FILE) → (c) contradiction check → (d) LÆX ratification + Kevin's stamp
- LÆX should run periodic queue review (~monthly or per major build merge)
- Ratified candidates get codified into the named canon doc and removed from this queue
- Rejected candidates stay in the queue with status `rejected` + rationale
