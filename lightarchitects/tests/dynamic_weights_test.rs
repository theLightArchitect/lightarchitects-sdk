//! Property tests for `SignalWeights::from_softmax` — Phase 4 Wave B.
//!
//! Validates the `from_softmax` invariants:
//! - weights always sum to exactly 100
//! - uniform logits produce near-equal (~25%) weights
//! - reasonable logits do not produce degenerate (>90%) single weights
//! - each component is individually bounded (≤100)

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_errors_doc,
    clippy::many_single_char_names
)]

use lightarchitects::helix::SignalWeights;

/// Softmax weights always sum to exactly 100.
#[test]
fn from_softmax_sums_to_100() {
    let cases: &[[f64; 4]] = &[
        [1.0, 1.0, 1.0, 1.0],   // uniform
        [10.0, 1.0, 0.5, 0.1],  // dominant first
        [0.1, 0.2, 0.3, 5.0],   // dominant last
        [-1.0, 0.0, 1.0, 2.0],  // mixed signs
        [100.0, 0.0, 0.0, 0.0], // extreme dominant
        [0.0, 0.0, 0.0, 0.0],   // all-zero (uniform softmax)
        [3.0, 2.0, 1.0, 0.5],   // descending
    ];
    for &logits in cases {
        let w = SignalWeights::from_softmax(logits);
        let (a, b, c, d) = w.as_percentages();
        let sum = u16::from(a) + u16::from(b) + u16::from(c) + u16::from(d);
        assert_eq!(
            sum, 100,
            "from_softmax({logits:?}) must sum to 100; got {sum} ({a}+{b}+{c}+{d})"
        );
    }
}

/// Uniform logits produce approximately equal weights (max deviation ≤ 1pp).
#[test]
fn from_softmax_uniform_logits_near_equal() {
    let w = SignalWeights::from_softmax([0.0, 0.0, 0.0, 0.0]);
    let (a, b, c, d) = w.as_percentages();
    for &pct in &[a, b, c, d] {
        assert!(
            (i16::from(pct) - 25).abs() <= 1,
            "uniform logits must produce near-25% weights; got {pct}"
        );
    }
}

/// Non-degeneracy: no single weight exceeds 90% for reasonable (non-extreme) inputs.
#[test]
fn from_softmax_non_degenerate_for_reasonable_inputs() {
    let cases: &[[f64; 4]] = &[
        [0.5, 1.0, 0.3, 0.8],
        [0.25, 0.35, 0.10, 0.30],
        [0.65, 0.25, 0.03, 0.07],
        [0.15, 0.30, 0.10, 0.45],
    ];
    for &logits in cases {
        let w = SignalWeights::from_softmax(logits);
        let (a, b, c, d) = w.as_percentages();
        for &pct in &[a, b, c, d] {
            assert!(
                pct <= 90,
                "reasonable logits {logits:?} must not produce degenerate weight >90%; got {pct}"
            );
        }
    }
}

/// Each component is individually bounded ≤100.
#[test]
fn from_softmax_each_component_bounded() {
    let w = SignalWeights::from_softmax([3.0, 2.0, 1.0, 0.5]);
    let (a, b, c, d) = w.as_percentages();
    for &pct in &[a, b, c, d] {
        assert!(pct <= 100, "each weight must be ≤100; got {pct}");
    }
}

/// Dominant logit receives the largest weight.
#[test]
fn from_softmax_dominant_logit_gets_largest_weight() {
    let w = SignalWeights::from_softmax([10.0, 1.0, 0.5, 0.1]);
    let (a, b, c, d) = w.as_percentages();
    assert!(
        a >= b && a >= c && a >= d,
        "dominant logit[0]=10.0 must yield the largest weight; got ({a},{b},{c},{d})"
    );
}
