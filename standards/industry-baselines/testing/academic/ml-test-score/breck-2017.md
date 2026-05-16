<!-- uuid: f1a8ae5b-e3e3-4807-9db2-db32d3c730ab -->
<!-- citation: Breck et al., IEEE Big Data 2017 | type: academic-foundation | re-pull: never -->
<!-- gate: [T], [Q] -->

# ML Test Score (Breck et al. 2017)

**Citation**: E. Breck, S. Cai, E. Nielsen, M. Salib, D. Sculley, "The ML Test Score: A Rubric for ML Production Readiness and Technical Debt Reduction," in *Proc. IEEE International Conference on Big Data*, pp. 1123–1132, 2017. Available: https://research.google/pubs/the-ml-test-score-a-rubric-for-ml-production-readiness-and-technical-debt-reduction/

## Verbatim quote (load-bearing)

> "Machine learning systems are notoriously difficult to test and maintain in production. The ML Test Score provides a 28-test rubric across four categories of ML system reliability, scoring readiness from 0 (very unreliable) to 7 (excellent)."

## The 4 categories × 28 tests

### Features and data
1. Feature expectations are captured in a schema
2. All features are beneficial
3. No feature's cost is too much
4. Features adhere to meta-level requirements (privacy, etc.)
5. The data pipeline has appropriate privacy controls
6. New features can be added quickly
7. All input feature code is tested

### Model development
1. Model specs are reviewed and submitted
2. Offline + online metrics correlate
3. All hyperparameters have been tuned
4. The impact of model staleness is known
5. A simpler model is not better
6. Model quality is sufficient on important data slices
7. The model is tested for considerations of inclusion (fairness, bias)

### ML infrastructure
1. Training is reproducible
2. Model specs are unit tested
3. The full ML pipeline is integration tested
4. Model quality is validated before serving
5. Model is debuggable
6. Models are canaried before serving
7. Serving models can be rolled back

### Monitoring tests for ML
1. Dependency changes result in notification
2. Data invariants hold in inputs
3. Training and serving are not skewed
4. Models are not too stale
5. Models are numerically stable
6. Computing performance has not regressed
7. Prediction quality has not regressed

## Scoring

Each test scored 0 (not done) / 0.5 (manually done) / 1.0 (automated). Sum across 28 = up to 28 points. Normalize to 0–7 scale by dividing by 4.

Threshold guidance (per paper):
- 0–1: very unreliable
- 1–2: ad hoc, untested ML
- 2–3: passes basic productionization tests
- 3–5: reasonably tested
- 5–7: exceptional levels of automated testing

## Why LASDLC LDB v1.0 cites this (D5 ML/AI quality)

§7.7 deliverable_benchmark D5 (Domain conditional) lists ML Test Score as the anchor for ai_ml_quality domain trigger (`risk_classification.ai_risk_tier ≠ none`). For LASDLC builds that ship ML systems:
- D5a — score against the 28-test rubric
- Goal: ≥3 (reasonably tested); ≥5 for production-critical ML
- Pairs with §6.5 selected_frameworks OWASP_LLM_Top_10 + MITRE_ATLAS for AI-system-specific security

## Status

- **Type**: academic paper; not maintained as a living spec
- **Re-pull cadence**: never (paper is cited as-is); follow-up Google ML reliability research cited separately
- **Used by**: LDB §7.7 D5 (Domain conditional, ai_ml_quality)
