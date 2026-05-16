<!-- uuid: 5ec6b420-83dd-49f2-9d59-b996b1c35721 -->
<!-- citation: Forsgren et al., ACM Queue 2021 | type: academic-foundation | re-pull: never -->
<!-- gate: [O] -->

# SPACE Framework (Forsgren et al. 2021)

**Citation**: N. Forsgren, M.-A. Storey, C. Maddila, T. Zimmermann, B. Houck, J. Butler, "The SPACE of Developer Productivity: There's more to it than you think," *Communications of the ACM* / *ACM Queue*, vol. 19, no. 1, pp. 20–48, January–February 2021.

## Verbatim quote (load-bearing)

> "Productivity cannot be reduced to a single metric or dimension. SPACE captures productivity as a multi-dimensional construct spanning Satisfaction & well-being, Performance, Activity, Communication & collaboration, and Efficiency & flow."

## The 5 dimensions

| Dimension | What it captures | Example metrics |
|-----------|------------------|-----------------|
| **S — Satisfaction & well-being** | How fulfilled, happy, healthy developers are with their work, team, tools, and culture | Survey ratings; retention rate; burnout signals |
| **P — Performance** | Outcome of a system or process | Code quality (defect density); customer satisfaction; reliability |
| **A — Activity** | Count of actions or outputs completed in the course of performing work | Number of design docs, PRs, commits, code reviews |
| **C — Communication & collaboration** | How people and teams communicate and work together | Discoverability of documentation; review timeliness; knowledge sharing |
| **E — Efficiency & flow** | Ability to complete work or make progress with minimal interruptions or delays | Flow state frequency; handoffs; perceived productivity |

## Why "SPACE" is anti-vanity-metrics

Forsgren et al. explicitly argue against single-metric productivity (e.g., "lines of code per day", "commits per week") because:
1. Optimizing one dimension often degrades others (e.g., maximizing Activity at expense of Satisfaction/Performance/Quality)
2. Subjective dimensions (Satisfaction, Communication) are real productivity factors and must be measured directly
3. Pairing objective + subjective measures yields a more honest signal than either alone

## Why LASDLC LDB v1.0 cites this (D8j)

§7.7 deliverable_benchmark D8j ("SPACE Performance & Efficiency-Flow composite score") composes:
- **D8d** (utilization, Flow Framework lens) → SPACE E (Efficiency & flow)
- **D8i** (operator satisfaction, Apdex-adapted) → SPACE S (Satisfaction proxy)
- **AYIN-derived interruption signal** (lack of context-switching) → SPACE E (Flow continuity)

D8j explicitly avoids treating Activity (PR count, commit count) as a primary productivity signal — per SPACE, that's the dimension most likely to mislead.

## Status

- **Type**: academic publication; not version-tracked as a living spec
- **Re-pull cadence**: never (paper is cited as-is); follow-up Forsgren publications cited separately if applicable
- **Used by**: LDB §7.7 D8j (SPACE composite)
