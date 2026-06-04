---
name: scope-fit-canon-template
description: "When applying a universal canon rule across heterogeneous artifacts, calibrate template by tier — full template for HIGH, compact for MEDIUM/LOW. Uniform template bloats lighter artifacts"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 767e46bb-eb90-4ad0-a585-e6f528850e34
---

When applying a new universal canon rule (e.g., Cookbook §82.3 5-touchpoint Contract Canon Integration section) across N artifacts of varying weight, calibrate the template depth to the artifact tier. The same rule with different elaboration:

- **HIGH-tier artifacts** (where the rule is load-bearing): full sectioned template (~30 lines) covering every required touchpoint
- **MEDIUM/LOW-tier artifacts** (where the rule applies but isn't structural): 1-paragraph compact integration (~3 lines) naming the governing contract + the 1-3 touchpoints that actually apply

**Why:** The rule itself is universal (every skill MUST respect contracts in its domain), but the depth of obligation varies per artifact. /BUILD has 5 distinct touchpoints with rich behavior — full template earns its lines. /ONBOARD has one (tour the canon during orientation) — forcing the 5-touchpoint shape produces "n/a" sections that don't earn their lines and dilute signal.

**How to apply:**

1. **Classify before authoring**: walk the artifact list and tier each one (BLOCKING / HIGH / MEDIUM / LOW). The cross-exam table from /REFLECT or /SCRUM already does this.
2. **Template per tier**:
   - **BLOCKING/HIGH**: full sectioned template covering every required touchpoint with skill-specific elaboration
   - **MEDIUM/LOW**: 1-paragraph integration naming (a) which contract governs the artifact, (b) which contract kinds it reads, (c) which touchpoints actually apply (1-3 of the 5)
3. **Sibling personas**: special case — get a tier-specific template emphasizing their Gatekeeper Registry gate ownership rather than the universal touchpoints
4. **Test for "this section has more nothing than something"**: if 3+ touchpoints would be "none" or "n/a", you're applying the wrong tier template

Pressure-tested 2026-06-04 Wave C (HIGH) vs Wave D (MEDIUM/LOW): Wave C added ~250 lines across 7 skills (avg 35 lines/skill, all earning their place); Wave D added ~103 lines across 16 artifacts (avg 6 lines/artifact). Forcing Wave-C-style uniform template across Wave D would have added ~560 lines of mostly-empty sections — 5.5× the actual delta — diluting the contract-canon integration signal across the skill ecosystem.

Generalizes beyond canon: any universal-rule application across heterogeneous artifacts. Documentation standards, lint configurations, instrumentation requirements — same calibration principle.
