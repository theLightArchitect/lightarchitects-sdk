<!-- uuid: a8fab7b0-e408-4cf9-897d-2cb076d044e7 -->
<!-- citation: Kelley & Walker, EJCC 1959 | type: academic-foundation | re-pull: never -->
<!-- source: academic — Kelley & Walker 1959; summary via Wikipedia + ACM Digital Library | version: original 1959 conference paper | scraped: 2026-05-04 | tool: WebFetch + canonical citation | re-pull: NEVER (foundational paper) -->
<!-- gate: [P] -->

# Critical Path Method (CPM) — Kelley & Walker 1959

**Citation**: Kelley, James E. Jr. and Walker, Morgan R. "Critical-path planning and scheduling," in *Papers presented at the December 1-3, 1959, Eastern Joint IRE-AIEE-ACM Computer Conference*, pp. 160–173, 1959. ACM Digital Library (DOI: 10.1145/1460299.1460318).

## Verbatim quote (load-bearing)

> "[CPM is] an algorithm for scheduling a set of project activities. A critical path represents the longest stretch of dependent activities and measuring the time required to complete them from start to finish."

## Historical origin

- **Developers**: Morgan R. Walker (DuPont) and James E. Kelley Jr. (Remington Rand)
- **Formal presentation**: December 1-3, 1959, Eastern Joint IRE-AIEE-ACM Computer Conference (Boston)
- **Publication**: Conference proceedings, 1959
- **Precursors**: DuPont's internal scheduling techniques (1940-1943, including Manhattan Project applications)
- **Sister technique**: PERT (Program Evaluation and Review Technique), developed contemporaneously by Booz Allen Hamilton and the U.S. Navy

## Core concept

CPM is **an algorithm for scheduling a set of project activities**. The critical path is the longest stretch of dependent activities; its total duration determines the minimum project completion time. Activities on the critical path have **zero total float** — any delay extends overall project duration.

## Mathematical formulation

### Inputs required
1. Complete activity inventory (typically via work breakdown structure)
2. Duration estimate for each activity
3. Predecessor/successor dependencies between activities
4. Logical endpoints (milestones, deliverables)

### Algorithm

**Forward pass** — compute earliest start (ES) and earliest finish (EF) for each activity:
- ES(a) = max{ EF(p) : p ∈ predecessors(a) }
- EF(a) = ES(a) + duration(a)

**Backward pass** — compute latest start (LS) and latest finish (LF):
- LF(a) = min{ LS(s) : s ∈ successors(a) }
- LS(a) = LF(a) − duration(a)

**Float (slack)**:
- Total Float(a) = LS(a) − ES(a) = LF(a) − EF(a)

**Critical path**: the set of activities where Total Float = 0.

## Critical path drag

For activities on the critical path:
- Activities without parallel work: drag = duration
- Activities with parallel activities: drag = min(duration, parallel-activity total-float)

Drag quantifies how much each critical activity is extending the project; it is the lever for compression analysis.

## Visualization

- **Activity-on-Arrow (AoA / PERT charts)**: historical; arrows = activities, nodes = events. Largely superseded.
- **Activity-on-Node (AoN)**: current standard; nodes = activities, arrows = precedence. Used in MS Project, Primavera, etc.

## Relationship to PERT

| | CPM | PERT |
|--|-----|------|
| Time treatment | deterministic (fixed durations) | probabilistic (3-point estimate: optimistic, most-likely, pessimistic) |
| Origin | DuPont/Remington Rand 1959 | US Navy 1958 (Polaris program) |
| Strength | precise scheduling, well-known durations | uncertainty quantification, novel projects |

Both techniques compose; modern PMs apply CPM logic with PERT-style estimates.

## Applications and historical milestones

- **1966**: First major use — World Trade Center Twin Towers construction
- Construction, aerospace, defense, software development, R&D, product development, engineering, maintenance
- Modern: Stanford's John Fondahl manual approach is the basis for current project management software (MS Project, Primavera P6, Smartsheet, Asana, etc.)

## Limitations

- Estimation variance — actual durations frequently deviate from planned
- Single-path optimization — does not natively handle resource constraints (CPM/Resource-leveling extensions exist)
- Float opacity — non-critical activities can become critical under perturbation; sensitivity analysis often needed

## Used by LASDLC

- **LDB D8g (Concurrency factor — DAG critical path)** — explicitly flagged in REGISTRY as "critical-path scheduling theory (academic, not yet pulled)"
- LASDLC wave-orchestration sequencing — agent dispatch order on multi-wave builds
- §7.7 D8 — wave throughput predicate uses critical-path-of-DAG analysis

## Re-pull policy

NEVER. This is a foundational paper; the algorithm has not changed since 1959. Modern derivatives (Goldratt's Critical Chain Method, resource-leveled CPM) are separate citations.
