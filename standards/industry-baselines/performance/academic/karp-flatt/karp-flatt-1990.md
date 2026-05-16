<!-- uuid: 8396cb33-2c83-441c-b9e0-6cce2232033f -->
<!-- citation: Karp & Flatt, CACM 1990 | type: academic-foundation | re-pull: never -->
<!-- gate: [P] -->

# Karp–Flatt Metric (1990)

**Citation**: A. H. Karp and H. P. Flatt, "Measuring parallel processor performance," *Communications of the ACM*, vol. 33, no. 5, pp. 539–543, May 1990.

## Verbatim quote (load-bearing)

> "the experimentally determined serial fraction provides a useful diagnostic of parallel program performance, complementing the standard speedup figure."

## Formula

The Karp–Flatt metric `e` (empirical serial fraction) is computed from observed speedup S and processor count N:

```
e = (1/S − 1/N) / (1 − 1/N)
```

Where:
- `S` = measured speedup
- `N` = number of processors used
- `e` = empirical (experimentally determined) serial fraction

## Why it's diagnostic

Amdahl's Law tells us the *theoretical* speedup ceiling assuming a known serial fraction `(1 − P)`. Karp–Flatt inverts the relationship: given observed S and N, what serial fraction WOULD have produced this result?

If the empirical serial fraction `e` increases as N grows, there is **hidden serialization** — overhead that wasn't in the original serial-fraction estimate. Common sources:
- Synchronization barriers
- Hand-off latency between agents/processes
- Resource contention (locks, I/O)
- Communication overhead growing super-linearly with N

If `e` is roughly constant as N grows, the parallelism is "honest" — observed performance matches Amdahl's prediction.

## Why LASDLC LDB v1.0 cites this (D8c)

§7.7 deliverable_benchmark D8c ("Empirical serial fraction") uses Karp–Flatt to diagnose parallel-agentic-orchestration performance. Goal: empirical `e` should approach the planned serial fraction recorded in the build's parallelism plan. Drift surfaces hidden bottlenecks (e.g., unplanned synchronization, hand-off bottleneck) that Amdahl's Law alone wouldn't catch.

Compose: D8b (Amdahl ceiling) gives the theoretical bound; D8c (Karp–Flatt) says whether the actual performance matches the bound.

## Status

- **Type**: foundational paper; not updated
- **Re-pull cadence**: never; cite original publication verbatim
- **Used by**: LDB §7.7 D8c
