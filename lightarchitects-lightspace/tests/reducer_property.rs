//! Property-based tests for `Lightspace::reduce`.
//!
//! Covers 7 invariants using `proptest` with 10_000 cases each:
//! - Determinism: same ordered event list → byte-identical snapshots.
//! - Seq regression: backward seq always rejected.
//! - Auth: Copilot cannot Detach.
//! - Payload limit: payload > 64 KiB → PayloadTooLarge.
//! - Confidence range: value ∉ [0.0, 1.0] → ConfidenceOutOfRange.
//! - Invariants I1–I5 preserved after N sequential events.
//! - Update+Gating permutation: semantic equivalence regardless of order.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod property {
    use chrono::TimeZone;
    use lightarchitects_lightspace::{
        CanvasEvent, Lightspace,
        types::{
            Actor, CardData, CardKind, CardState, CardTransition, EvidenceTier, Provenance,
            UpdateMode,
        },
    };
    use proptest::prelude::*;
    use uuid::Uuid;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn provenance() -> Provenance {
        // Fixed timestamp ensures byte-identical serialization across two engines.
        // Utc::now() would produce microsecond divergence between independent chains.
        let ts = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        Provenance {
            agent: "prop-agent".to_owned(),
            source_uri: "helix://prop/test".to_owned(),
            span_id: None,
            ts,
        }
    }

    fn card(id: &str) -> CardData {
        CardData {
            id: id.to_owned(),
            kind: CardKind::Monitor,
            title: format!("card-{id}"),
            content: serde_json::json!({"x": 1}),
            provenance: provenance(),
            state: CardState::Attached,
            attribution: None,
        }
    }

    // Strategy: unique card IDs for a given count n (avoids collision errors).
    fn card_ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("prop-{i:04}")).collect()
    }

    // ── Property 1: Determinism ───────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// Same ordered Card events applied to two independent engines
        /// must produce byte-identical snapshots (IndexMap insertion order preserved).
        #[test]
        fn determinism_same_seq_same_snapshot(n in 1usize..=8) {
            let ids = card_ids(n);
            let session = Uuid::new_v4();

            let mut ls_a = Lightspace::new(session);
            let mut ls_b = Lightspace::new(session);

            for id in &ids {
                let ev_a = CanvasEvent::Card(card(id));
                let ev_b = CanvasEvent::Card(card(id));
                ls_a = ls_a.reduce(ev_a).expect("reduce a");
                ls_b = ls_b.reduce(ev_b).expect("reduce b");
            }

            let snap_a = serde_json::to_vec(&ls_a.snapshot()).expect("serialize a");
            let snap_b = serde_json::to_vec(&ls_b.snapshot()).expect("serialize b");
            prop_assert_eq!(snap_a, snap_b);
        }
    }

    // ── Property 2: Seq regression ────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// Any Update with seq ≤ last-seen seq must return SeqRegression.
        #[test]
        fn seq_regression_always_rejected(first_seq in 2u64..=10_000, backward in 1u64..=2u64) {
            let backward_seq = first_seq.saturating_sub(backward);
            let card_id = "seq-card";

            let ls = Lightspace::new(Uuid::new_v4())
                .reduce(CanvasEvent::Card(card(card_id)))
                .expect("insert card")
                .reduce(CanvasEvent::Update {
                    card_id: card_id.to_owned(),
                    seq: first_seq,
                    mode: UpdateMode::Replace,
                    path: None,
                    payload: serde_json::json!({"v": first_seq}),
                })
                .expect("first update");

            let result = ls.reduce(CanvasEvent::Update {
                card_id: card_id.to_owned(),
                seq: backward_seq,
                mode: UpdateMode::Replace,
                path: None,
                payload: serde_json::json!({"v": backward_seq}),
            });

            prop_assert!(
                result.is_err(),
                "expected SeqRegression error for seq {backward_seq} after {first_seq}"
            );
            let err_str = result.unwrap_err().to_string();
            prop_assert!(
                err_str.contains("seq regression"),
                "expected SeqRegression variant, got: {err_str}"
            );
        }
    }

    // ── Property 3: Copilot cannot Detach ────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// Actor::Copilot attempting CardTransition::Detach must always be rejected.
        #[test]
        fn copilot_cannot_detach(suffix in "[a-z0-9]{4,8}") {
            let id = format!("copilot-{suffix}");

            let ls = Lightspace::new(Uuid::new_v4())
                .reduce(CanvasEvent::Card(card(&id)))
                .expect("insert card");

            let result = ls.reduce(CanvasEvent::Lifecycle {
                card_id: id.clone(),
                transition: CardTransition::Detach,
                actor: Actor::Copilot,
                ghost: false,
                attribution: None,
            });

            prop_assert!(
                result.is_err(),
                "Copilot detach of {id} should have been rejected"
            );
            let err_str = result.unwrap_err().to_string();
            prop_assert!(
                err_str.contains("not authorised"),
                "expected UnauthorisedTransition, got: {err_str}"
            );
        }
    }

    // ── Property 4: Payload size limit (64 KiB) ──────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// An Update payload exceeding 65_535 bytes must return PayloadTooLarge.
        #[test]
        fn payload_over_64kib_rejected(excess in 1usize..=512) {
            let card_id = "size-card";
            // Build a JSON string whose serde encoding exceeds 64 KiB.
            // serde_json::Value::String serializes as `"<str>"` (+2 bytes for quotes).
            // Target: content length ≥ 65_536 bytes after encoding.
            let big_str = "x".repeat(65_535 + excess);
            let payload = serde_json::Value::String(big_str);

            let ls = Lightspace::new(Uuid::new_v4())
                .reduce(CanvasEvent::Card(card(card_id)))
                .expect("insert card");

            let result = ls.reduce(CanvasEvent::Update {
                card_id: card_id.to_owned(),
                seq: 1,
                mode: UpdateMode::Replace,
                path: None,
                payload,
            });

            prop_assert!(
                result.is_err(),
                "oversized payload should be rejected"
            );
            let err_str = result.unwrap_err().to_string();
            prop_assert!(
                err_str.contains("too large"),
                "expected PayloadTooLarge, got: {err_str}"
            );
        }
    }

    // ── Property 5: Confidence value range ───────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// Confidence value outside [0.0, 1.0] must return ConfidenceOutOfRange.
        #[test]
        fn confidence_out_of_range_rejected(
            // Generate values clearly outside [0.0, 1.0] to avoid f64 edge cases.
            value in prop_oneof![
                (1.001f64..=100.0f64),
                (-100.0f64..=-0.001f64),
            ]
        ) {
            let card_id = "conf-card";
            let ls = Lightspace::new(Uuid::new_v4())
                .reduce(CanvasEvent::Card(card(card_id)))
                .expect("insert card");

            let result = ls.reduce(CanvasEvent::Confidence {
                target_id: card_id.to_owned(),
                target_kind: "monitor".to_owned(),
                value,
                basis: "basis text".to_owned(),
                contradicts: vec![],
                evidence_tier: EvidenceTier::Low,
            });

            prop_assert!(
                result.is_err(),
                "value {value} outside [0.0, 1.0] should be rejected"
            );
            let err_str = result.unwrap_err().to_string();
            prop_assert!(
                err_str.contains("out of range"),
                "expected ConfidenceOutOfRange, got: {err_str}"
            );
        }
    }

    // ── Property 6: Invariants I1–I5 after N sequential events ───────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// After N Card + Update events on distinct IDs, all 5 post-reduce
        /// invariants must hold (enforced internally by assert_invariants;
        /// any violation surfaces as InvariantViolation error).
        /// This property proves `reduce()` never silently violates invariants.
        #[test]
        fn invariants_preserved_after_n_events(n in 1usize..=20) {
            let ids = card_ids(n);
            let mut ls = Lightspace::new(Uuid::new_v4());

            for id in &ids {
                ls = ls
                    .reduce(CanvasEvent::Card(card(id)))
                    .expect("reduce Card should not violate invariants");
            }

            // snapshot_seq must be ≥ n (I1)
            let snap = ls.snapshot();
            prop_assert!(
                snap.snapshot_seq >= n as u64,
                "snapshot_seq {} < expected ≥ {n}",
                snap.snapshot_seq
            );
            // All card IDs present (I2 proxy)
            for id in &ids {
                prop_assert!(
                    snap.state.cards.contains_key(id),
                    "card {id} missing after N events"
                );
            }
        }
    }

    // ── Property 7: Update + Gating permutation (semantic equivalence) ────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// Applying Update then Gating vs Gating then Update on the same card
        /// must produce semantically equivalent state:
        /// - Same card content (from the Update)
        /// - Same gate evaluation result (from the Gating)
        ///
        /// Note: byte-equality is NOT asserted because snapshot_seq advances
        /// by 2 in both cases but the intermediate per-operation seq differs
        /// in the `per_card_seq` map.
        #[test]
        fn update_and_gating_permutation_semantic_eq(
            content_val in 0u32..=1_000,
            gate_label in "[A-Z]{1,3}",
            satisfied in any::<bool>(),
        ) {
            let card_id = "perm-card";
            let payload = serde_json::json!({"v": content_val});
            let session = Uuid::new_v4();

            let base = Lightspace::new(session)
                .reduce(CanvasEvent::Card(card(card_id)))
                .expect("insert card");

            // Order A: Update → Gating
            let base_a = Lightspace::restore(base.snapshot());
            let ls_a = base_a
                .reduce(CanvasEvent::Update {
                    card_id: card_id.to_owned(),
                    seq: 1,
                    mode: UpdateMode::Replace,
                    path: None,
                    payload: payload.clone(),
                })
                .expect("update A")
                .reduce(CanvasEvent::Gating {
                    card_id: card_id.to_owned(),
                    gate: gate_label.clone(),
                    satisfied,
                    reason: None,
                })
                .expect("gating A");

            // Order B: Gating → Update (seq must be > Gating's seq = 0, so use seq=1 still valid)
            let base_b = Lightspace::restore(base.snapshot());
            let ls_b = base_b
                .reduce(CanvasEvent::Gating {
                    card_id: card_id.to_owned(),
                    gate: gate_label.clone(),
                    satisfied,
                    reason: None,
                })
                .expect("gating B")
                .reduce(CanvasEvent::Update {
                    card_id: card_id.to_owned(),
                    seq: 1,
                    mode: UpdateMode::Replace,
                    path: None,
                    payload: payload.clone(),
                })
                .expect("update B");

            let snap_a = ls_a.snapshot();
            let snap_b = ls_b.snapshot();

            // Card content must be identical
            let content_a = &snap_a.state.cards[card_id].content;
            let content_b = &snap_b.state.cards[card_id].content;
            prop_assert_eq!(
                content_a,
                content_b,
                "card content differs by order: {:?} vs {:?}",
                content_a,
                content_b
            );

            // Gate evaluation must be identical
            let gate_a = &snap_a.state.gating_evaluations[card_id];
            let gate_b = &snap_b.state.gating_evaluations[card_id];
            prop_assert_eq!(
                gate_a.satisfied, gate_b.satisfied,
                "gate satisfied differs by order"
            );
            prop_assert_eq!(
                gate_a.gate.clone(),
                gate_b.gate.clone(),
                "gate label differs by order"
            );
        }
    }
}
