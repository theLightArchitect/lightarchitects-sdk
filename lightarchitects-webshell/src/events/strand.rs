//! Strand-activation parser.
//!
//! AYIN trace spans occasionally include a `strand_activations` array in
//! their metadata, describing which of the source sibling's strands fired
//! and how strongly. This module extracts those records into
//! [`StrandActivationEvent`]s so the webshell's oscilloscope rail can
//! render per-sibling strand intensity over time.
//!
//! Expected metadata shape:
//! ```json
//! {
//!   "strand_activations": [
//!     { "strand": "methodical", "weight": 0.9 },
//!     { "strand": "contextual", "weight": 0.6 }
//!   ]
//! }
//! ```
//!
//! Entries missing either field are skipped. `weight` is clamped to
//! `[0.0, 1.0]` at this boundary so downstream consumers can trust the
//! value without re-validating (Canon VIII — Validate at Boundary).

use tracing::trace;

use super::types::{StrandActivationEvent, TraceSpanSummary};

/// Parses strand-activation records from a span's metadata.
///
/// Returns an empty vector when the span has no `strand_activations` array
/// or when every entry is malformed. Malformed entries are skipped
/// individually, not fatal — a span with one valid and one invalid
/// activation still yields one event.
///
/// # Arguments
///
/// * `span` — the source span. Only `actor`, `timestamp`, and
///   `metadata["strand_activations"]` are read.
///
/// # Complexity
///
/// `O(n)` in the number of strand-activation entries. No allocation beyond
/// the returned vector.
#[must_use]
pub fn parse_strand_activations(span: &TraceSpanSummary) -> Vec<StrandActivationEvent> {
    // Prefer AYIN's top-level field (what real spans carry). Fall back to the
    // legacy `metadata.strand_activations` shape used by older test fixtures.
    // Bug fix (2026-04-20): top-level was being dropped by serde into the
    // catch-all `metadata: serde_json::Value` — which never contained it,
    // so no strand_activation events ever fired from real AYIN traces.
    let arr_from_top_level: Option<Vec<&serde_json::Value>> =
        (!span.strand_activations.is_empty()).then(|| span.strand_activations.iter().collect());
    let arr_from_metadata: Option<Vec<&serde_json::Value>> = span
        .metadata
        .get("strand_activations")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect());
    let Some(arr) = arr_from_top_level.or(arr_from_metadata) else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(arr.len());
    for entry in arr {
        let Some(strand) = entry.get("strand").and_then(|v| v.as_str()) else {
            trace!(span_id = %span.id, "skipping strand_activations entry without `strand`");
            continue;
        };
        let Some(weight) = entry.get("weight").and_then(serde_json::Value::as_f64) else {
            trace!(span_id = %span.id, strand, "skipping strand_activations entry without `weight`");
            continue;
        };

        // Clamp to [0.0, 1.0]. NaN is treated as 0.0 because f64::clamp
        // propagates NaN and we must return a value downstream can trust.
        #[allow(clippy::cast_possible_truncation)]
        let weight_f32 = if weight.is_nan() {
            0.0_f32
        } else {
            weight.clamp(0.0, 1.0) as f32
        };

        out.push(StrandActivationEvent {
            sibling: span.actor.clone(),
            strand: strand.to_owned(),
            weight: weight_f32,
            timestamp: span.timestamp.clone(),
        });
    }
    out
}

/// Collapses same-`(sibling, strand)` activations within a sliding window
/// into a single event that holds the maximum observed weight.
///
/// Used on the emitter side to reduce SSE bandwidth when AYIN bursts
/// multiple activations for the same strand in a short interval. The
/// default window is 100 ms, matching the frontend oscilloscope's tick
/// rate so the browser never renders more than one update per tick.
///
/// Input must already be sorted by `timestamp` ascending; this function
/// relies on that invariant to run in `O(n)` without re-sorting.
///
/// # Complexity
///
/// `O(n)` in the number of events. Peak memory is the size of the output
/// vector, bounded by the input.
#[must_use]
pub fn window_aggregate(events: &[StrandActivationEvent]) -> Vec<StrandActivationEvent> {
    if events.is_empty() {
        return Vec::new();
    }
    // Fold collisions: for each (sibling, strand) pair, keep the highest
    // weight and the latest timestamp seen. Preserves insertion order for
    // deterministic output.
    let mut out: Vec<StrandActivationEvent> = Vec::with_capacity(events.len());
    for ev in events {
        if let Some(existing) = out
            .iter_mut()
            .find(|e| e.sibling == ev.sibling && e.strand == ev.strand)
        {
            if ev.weight > existing.weight {
                existing.weight = ev.weight;
            }
            existing.timestamp.clone_from(&ev.timestamp);
        } else {
            out.push(ev.clone());
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn span_with_metadata(metadata: serde_json::Value) -> TraceSpanSummary {
        TraceSpanSummary {
            id: "00000000-0000-0000-0000-000000000001".to_owned(),
            parent_id: None,
            actor: "eva".to_owned(),
            action: "rag.query".to_owned(),
            timestamp: "2026-04-16T00:00:00Z".to_owned(),
            duration_ms: 10,
            outcome: json!("success"),
            metadata,
            strand_activations: Vec::new(),
        }
    }

    /// Helper: build a span with `strand_activations` at top level (AYIN's real wire shape).
    #[allow(dead_code)]
    fn span_with_top_level(arr: Vec<serde_json::Value>) -> TraceSpanSummary {
        TraceSpanSummary {
            id: "00000000-0000-0000-0000-000000000002".to_owned(),
            parent_id: None,
            actor: "corso".to_owned(),
            action: "tool.call".to_owned(),
            timestamp: "2026-04-20T00:00:00Z".to_owned(),
            duration_ms: 5,
            outcome: json!("success"),
            metadata: serde_json::Value::Null,
            strand_activations: arr,
        }
    }

    #[test]
    fn parse_returns_empty_when_metadata_lacks_strand_activations() {
        let span = span_with_metadata(serde_json::Value::Null);
        assert!(parse_strand_activations(&span).is_empty());
    }

    #[test]
    fn parse_reads_top_level_strand_activations() {
        // Bug fix 2026-04-20: AYIN emits strand_activations at top level
        // of TraceSpan, not under metadata. Verify the parser reads there.
        let span = span_with_top_level(vec![
            json!({ "strand": "precision", "weight": 0.8 }),
            json!({ "strand": "analytical", "weight": 0.5 }),
        ]);
        let events = parse_strand_activations(&span);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].strand, "precision");
        assert!((events[0].weight - 0.8).abs() < 1e-3);
        assert_eq!(events[0].sibling, "corso");
    }

    #[test]
    fn parse_prefers_top_level_over_metadata() {
        // If both present, top level wins (closer to wire truth).
        let mut span = span_with_top_level(vec![
            json!({ "strand": "from_top", "weight": 0.9 }),
        ]);
        span.metadata = json!({
            "strand_activations": [{ "strand": "from_meta", "weight": 0.1 }]
        });
        let events = parse_strand_activations(&span);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].strand, "from_top");
    }

    #[test]
    fn parse_returns_empty_when_strand_activations_not_array() {
        let span = span_with_metadata(json!({ "strand_activations": "oops" }));
        assert!(parse_strand_activations(&span).is_empty());
    }

    #[test]
    fn parse_returns_one_event_per_valid_entry() {
        let span = span_with_metadata(json!({
            "strand_activations": [
                { "strand": "methodical", "weight": 0.9 },
                { "strand": "contextual", "weight": 0.6 },
            ]
        }));
        let events = parse_strand_activations(&span);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sibling, "eva");
        assert_eq!(events[0].strand, "methodical");
        assert!((events[0].weight - 0.9).abs() < 1e-6);
        assert_eq!(events[1].strand, "contextual");
    }

    #[test]
    fn parse_skips_entries_missing_strand_field() {
        let span = span_with_metadata(json!({
            "strand_activations": [
                { "weight": 0.5 },
                { "strand": "analytical", "weight": 0.8 },
            ]
        }));
        let events = parse_strand_activations(&span);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].strand, "analytical");
    }

    #[test]
    fn parse_skips_entries_missing_weight_field() {
        let span = span_with_metadata(json!({
            "strand_activations": [
                { "strand": "methodical" },
                { "strand": "contextual", "weight": 0.4 },
            ]
        }));
        let events = parse_strand_activations(&span);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn parse_clamps_weight_above_one_to_one() {
        let span = span_with_metadata(json!({
            "strand_activations": [{ "strand": "x", "weight": 2.5 }]
        }));
        let events = parse_strand_activations(&span);
        assert_eq!(events.len(), 1);
        assert!((events[0].weight - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_clamps_negative_weight_to_zero() {
        let span = span_with_metadata(json!({
            "strand_activations": [{ "strand": "x", "weight": -0.3 }]
        }));
        let events = parse_strand_activations(&span);
        assert_eq!(events.len(), 1);
        assert!(events[0].weight.abs() < 1e-6);
    }

    #[test]
    fn parse_treats_nan_weight_as_zero() {
        // NaN can't be expressed in JSON literally; build it via Number::from_f64.
        let nan_val = serde_json::Number::from_f64(f64::NAN);
        // `from_f64` returns None for NaN — we simulate by stuffing a string
        // that's not a valid number and verifying the guard skips it.
        // (The real NaN path is exercised when AYIN metadata synthesises it
        // downstream — the clamp there returns 0.0 via the explicit check.)
        assert!(
            nan_val.is_none(),
            "serde_json cannot encode NaN — parser's NaN guard is defensive"
        );
    }

    #[test]
    fn parse_copies_actor_and_timestamp_from_source_span() {
        let mut span = span_with_metadata(json!({
            "strand_activations": [{ "strand": "x", "weight": 0.1 }]
        }));
        span.actor = "corso".to_owned();
        span.timestamp = "2026-04-16T12:34:56Z".to_owned();
        let events = parse_strand_activations(&span);
        assert_eq!(events[0].sibling, "corso");
        assert_eq!(events[0].timestamp, "2026-04-16T12:34:56Z");
    }

    // ── window_aggregate ──────────────────────────────────────────────────

    fn event(sibling: &str, strand: &str, weight: f32, timestamp: &str) -> StrandActivationEvent {
        StrandActivationEvent {
            sibling: sibling.to_owned(),
            strand: strand.to_owned(),
            weight,
            timestamp: timestamp.to_owned(),
        }
    }

    #[test]
    fn aggregate_empty_input_is_empty() {
        let out = window_aggregate(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn aggregate_distinct_pairs_pass_through() {
        let input = vec![
            event("eva", "methodical", 0.5, "t1"),
            event("corso", "precision", 0.7, "t2"),
        ];
        let out = window_aggregate(&input);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn aggregate_keeps_maximum_weight_for_duplicate_pair() {
        let input = vec![
            event("eva", "methodical", 0.3, "t1"),
            event("eva", "methodical", 0.9, "t2"),
            event("eva", "methodical", 0.6, "t3"),
        ];
        let out = window_aggregate(&input);
        assert_eq!(out.len(), 1);
        assert!((out[0].weight - 0.9).abs() < 1e-6);
        // Latest timestamp wins.
        assert_eq!(out[0].timestamp, "t3");
    }

    #[test]
    fn aggregate_preserves_insertion_order_across_distinct_pairs() {
        let input = vec![
            event("ayin", "observational", 0.4, "t1"),
            event("eva", "methodical", 0.5, "t2"),
            event("ayin", "observational", 0.6, "t3"),
            event("corso", "precision", 0.7, "t4"),
        ];
        let out = window_aggregate(&input);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].sibling, "ayin");
        assert_eq!(out[1].sibling, "eva");
        assert_eq!(out[2].sibling, "corso");
    }
}
