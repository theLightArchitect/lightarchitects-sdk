<!-- uuid: 20aac364-b451-48b2-a11c-193c02d2574a -->
<!-- citation: Amdahl, AFIPS 1967 | type: academic-foundation | re-pull: never -->
<!-- gate: [P] -->

# Amdahl's Law (1967)

**Citation**: G. M. Amdahl, "Validity of the single processor approach to achieving large scale computing capabilities," in *Proceedings of the AFIPS Spring Joint Computer Conference*, vol. 30, pp. 483–485, 1967.

## Verbatim quote (load-bearing)

> "the overall performance improvement gained by optimizing a single part of a system is limited by the fraction of time that the improved part is actually used."

## Formula

```
S(N) = 1 / ((1 − P) + P/N)
```

Where:
- `S(N)` = speedup with N processors
- `P` = parallelizable fraction of the program (0 ≤ P ≤ 1)
- `(1 − P)` = serial fraction
- `N` = number of processors

## Theoretical ceiling

As N → ∞, speedup approaches the ceiling:
```
S(∞) = 1 / (1 − P)
```

So if P = 0.95 (95% parallelizable), maximum achievable speedup is 20× regardless of how many processors are added. The serial fraction is the structural bottleneck.

## Why LASDLC LDB v1.0 cites this (D8b)

§7.7 deliverable_benchmark D8b ("Parallel speedup achieved") evaluates whether observed speedup approaches the Amdahl ceiling for the planned parallelizable fraction. Goal: S ≥ 0.7 × Amdahl ceiling.

Companion: §7.7 D8c (Karp–Flatt empirical serial fraction) — measures observed serial fraction post-hoc; if Karp–Flatt > planned (1 − P), there is hidden serialization beyond what Amdahl predicted (e.g., unplanned hand-off bottleneck).

## Status

- **Type**: foundational paper; not updated
- **Re-pull cadence**: never; cite original publication verbatim
- **Used by**: LDB §7.7 D8b
