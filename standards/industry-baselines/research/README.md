<!-- uuid: research-folder-root — folder README, no document UUID -->
<!-- gate: [R] -->

# Research + Risk — Industry Baselines

**Gate**: `[R]` Research + Risk  
**Gatekeeper**: `lightarchitects:researcher` (QUANTUM)  
**Veto authority**: None (blocking gate via normal verdict aggregation)

This folder holds standards and methodologies anchoring QUANTUM's `[R]` gate
scoring: BCRA blast score analysis, dependency risk surface assessments, and
evidence chain review frameworks.

## Standards to populate

| Standard | Source | Priority |
|----------|--------|---------|
| BCRA (Blast Consequence + Risk Assessment) | LA-internal | P0 — primary scoring anchor |
| OWASP Dependency Check methodology | OWASP | P1 |
| CycloneDX SBOM specification | CycloneDX | P1 |
| NIST SP 800-161 (supply chain risk) | NIST | P2 |
| MITRE ATT&CK for supply chain | MITRE | P2 |

## Scoring anchor

QUANTUM fires the `[R]` gate at every phase boundary using:
1. `sibling: "quantum"` `action: "scan"` — evidence-based diff analysis
2. `sibling: "quantum"` `action: "research"` — BCRA blast score per boundary
3. `sibling: "soul"` `action: "search"` — prior incident/decision retrieval from helix

**FAIL trigger**: Any dependency, binary, API, config, or coverage boundary
scores CRITICAL on the BCRA blast score scale.

## Population status

SCAFFOLDED — awaiting BCRA methodology document from LA-internal canon.
