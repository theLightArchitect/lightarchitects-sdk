<!-- uuid: 13b48bd7-9523-4ab8-b040-c1c41e2c84a9 -->
<!-- citation: Kersten, IT Revolution 2018 | type: academic-foundation | re-pull: never -->
<!-- gate: [O], [P] -->

# Flow Framework (Mik Kersten 2018)

**Citation**: M. Kersten, *Project to Product: How to Survive and Thrive in the Age of Digital Disruption with the Flow Framework*. Portland, OR: IT Revolution, 2018.

## Verbatim quote (load-bearing)

> "Optimizing for project completion misses the point. Software is not a project; it is a continuous flow of value. The Flow Framework measures the flow of business value through the value stream — not project tasks, not story points, not lines of code."

## The 4 Flow Items (what flows)

Kersten defines four mutually-exclusive types of work that traverse the value stream:

| Flow Item | Definition | Example |
|-----------|-----------|---------|
| **Features** | New value being added | New product capabilities, user-facing improvements |
| **Defects** | Quality issues being fixed | Bug fixes, regression corrections |
| **Debt** | Reducing future delivery friction | Refactoring, infrastructure improvements |
| **Risks** | Addressing security, compliance, governance | Security patches, audit-required changes |

Track distribution: e.g., 60% Features / 20% Defects / 15% Debt / 5% Risks. Imbalance signals trouble (e.g., Defects rising → quality erosion; Risks accumulating → compliance debt).

## The 4 Flow Metrics (how flow is measured)

| Metric | Definition |
|--------|-----------|
| **Flow Velocity** | Number of flow items completed in a given period |
| **Flow Time** | End-to-end time from "started" to "delivered" (cycle time) |
| **Flow Efficiency** | Active work time / total elapsed time (waste detector) |
| **Flow Load** | Number of flow items in progress (WIP) |

## Why Flow Efficiency is load-bearing

Flow Efficiency = active_time / total_time. Industry observation (per Kersten): typical knowledge work has ≤15% flow efficiency — 85% of cycle time is waiting (queues, context switches, hand-offs). Improving efficiency from 15% to 30% is more impactful than 2× more developers.

## Why LASDLC LDB v1.0 cites this (D8a, D8d)

**D8a (End-to-end delivery time)**:
- Flow Time directly = LDB D8a (idea → ratified plan → first deploy → operator-validated)
- Per-tier baseline targets (SMALL <8h, MEDIUM <40h, LARGE <160h) are Flow Time targets

**D8d (Agent utilization / Flow Efficiency)**:
- Flow Framework Flow Efficiency = LDB D8d
- Goal ≥0.70 (industry rule of thumb for knowledge work — already 4-5× typical baseline of 0.15)

Composes with Little's Law (Flow Load = Flow Velocity × Flow Time) for D8h wave-boundary throughput evaluation.

## Status

- **Type**: book + Tasktop methodology; not an open standard
- **Re-pull cadence**: never (book content stable); follow-up Tasktop / Planview research cited separately
- **Used by**: LDB §7.7 D8a (Flow Time) + D8d (Flow Efficiency)
