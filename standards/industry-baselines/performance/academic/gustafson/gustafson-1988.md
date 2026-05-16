<!-- uuid: ef54e129-4fb5-44a3-b3db-fc4c6b909769 -->
<!-- citation: Gustafson, CACM 1988 | type: academic-foundation | re-pull: never -->
<!-- gate: [P] -->

# Gustafson's Law (1988)

**Citation**: J. L. Gustafson, "Reevaluating Amdahl's Law," *Communications of the ACM*, vol. 31, no. 5, pp. 532–533, May 1988.

## Verbatim quote (load-bearing)

> "speedup should be measured by scaling the problem to the number of processors, not by fixing the problem size."

## Formula

```
S(N) = N − α · (N − 1)
```

Where:
- `S(N)` = scaled speedup with N processors
- `α` = serial fraction (with the larger problem)
- `N` = number of processors

Equivalently:
```
S(N) = α + (1 − α) · N    (where α is serial fraction)
```

## Why it complements Amdahl

Amdahl assumes problem size is FIXED; Gustafson assumes problem size SCALES with the number of processors. In practice for many workloads (data-parallel, divide-and-conquer), the serial fraction stays roughly constant in time as more processors are added because each processor handles more data — so scaled speedup grows linearly with N rather than saturating at the Amdahl ceiling.

## Why LASDLC LDB v1.0 cites this (D8b)

§7.7 deliverable_benchmark D8b lists Gustafson alongside Amdahl as the "scalable parallelism" lens for problems where work grows with parallelism. For LASDLC build pipelines this matters when the agentic team scales to handle a larger build scope (e.g., parallel-agent code review across more files); Gustafson predicts better-than-Amdahl scaling on those workloads.

## Status

- **Type**: foundational paper; not updated
- **Re-pull cadence**: never; cite original publication verbatim
- **Used by**: LDB §7.7 D8b (alongside Amdahl)
