//! Integration tests: reducer round-trips and key invariant checks.

#[allow(clippy::unwrap_used, clippy::expect_used)]
mod round_trip {
    use lightarchitects_lightspace::types::{
        Actor, CardData, CardKind, CardState, CardTransition, DrawerFileAction, DrawerFileData,
        EvidenceTier, Provenance, UpdateMode,
    };
    use lightarchitects_lightspace::{CanvasEvent, Lightspace};
    use uuid::Uuid;

    fn test_session() -> Uuid {
        Uuid::new_v4()
    }

    fn test_provenance() -> Provenance {
        Provenance {
            agent: "test-agent".to_owned(),
            source_uri: "helix://test/entry".to_owned(),
            span_id: None,
            ts: chrono::Utc::now(),
        }
    }

    fn test_card(id: &str, kind: CardKind) -> CardData {
        CardData {
            id: id.to_owned(),
            kind,
            title: format!("Test {id}"),
            content: serde_json::json!({}),
            provenance: test_provenance(),
            state: CardState::Attached,
            attribution: None,
        }
    }

    #[test]
    fn new_canvas_has_zero_seq() {
        let ls = Lightspace::new(test_session());
        assert_eq!(ls.state.snapshot_seq, 0);
        assert!(ls.state.cards.is_empty());
    }

    #[test]
    fn card_event_increments_seq_and_inserts_card() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Monitor)))
            .unwrap();
        assert_eq!(ls.state.snapshot_seq, 1);
        assert!(ls.state.cards.contains_key("c1"));
        assert_eq!(ls.state.cards["c1"].state, CardState::Attached);
    }

    #[test]
    fn duplicate_card_id_returns_collision_error() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Monitor)))
            .unwrap();
        let err = ls
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Bash)))
            .unwrap_err();
        assert!(matches!(
            err,
            lightarchitects_lightspace::error::ReducerError::CardIdCollision(_)
        ));
    }

    #[test]
    fn update_replace_changes_content() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Thinking)))
            .unwrap()
            .reduce(CanvasEvent::Update {
                card_id: "c1".to_owned(),
                seq: 1,
                mode: UpdateMode::Replace,
                path: None,
                payload: serde_json::json!({"text": "hello"}),
            })
            .unwrap();
        assert_eq!(
            ls.state.cards["c1"].content,
            serde_json::json!({"text": "hello"})
        );
        assert_eq!(*ls.state.per_card_seq.get("c1").unwrap(), 1u64);
    }

    #[test]
    fn update_seq_regression_returns_error() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Thinking)))
            .unwrap()
            .reduce(CanvasEvent::Update {
                card_id: "c1".to_owned(),
                seq: 5,
                mode: UpdateMode::Replace,
                path: None,
                payload: serde_json::json!({"v": 5}),
            })
            .unwrap();
        let err = ls
            .reduce(CanvasEvent::Update {
                card_id: "c1".to_owned(),
                seq: 3,
                mode: UpdateMode::Replace,
                path: None,
                payload: serde_json::json!({"v": 3}),
            })
            .unwrap_err();
        assert!(matches!(
            err,
            lightarchitects_lightspace::error::ReducerError::SeqRegression { .. }
        ));
    }

    #[test]
    fn lifecycle_detach_creates_tombstone_when_ghost() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Artifact)))
            .unwrap()
            .reduce(CanvasEvent::Lifecycle {
                card_id: "c1".to_owned(),
                transition: CardTransition::Detach,
                actor: Actor::Operator,
                ghost: true,
                attribution: None,
            })
            .unwrap();
        assert_eq!(ls.state.tombstones.len(), 1);
        assert_eq!(ls.state.tombstones[0].card_id, "c1");
        assert_eq!(ls.state.cards["c1"].state, CardState::Detached);
    }

    #[test]
    fn copilot_cannot_detach() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Artifact)))
            .unwrap();
        let err = ls
            .reduce(CanvasEvent::Lifecycle {
                card_id: "c1".to_owned(),
                transition: CardTransition::Detach,
                actor: Actor::Copilot,
                ghost: false,
                attribution: None,
            })
            .unwrap_err();
        assert!(matches!(
            err,
            lightarchitects_lightspace::error::ReducerError::UnauthorisedTransition { .. }
        ));
    }

    #[test]
    fn snapshot_restore_round_trip() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Research)))
            .unwrap();
        let snap = ls.snapshot();
        let bytes = snap.to_bytes().unwrap();
        let snap2 = lightarchitects_lightspace::snapshot::Snapshot::from_bytes(&bytes).unwrap();
        let ls2 = Lightspace::restore(snap2);
        assert_eq!(ls2.state.snapshot_seq, ls.state.snapshot_seq);
        assert_eq!(ls2.state.cards.len(), ls.state.cards.len());
    }

    #[test]
    fn confidence_basis_too_short_returns_error() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card(test_card("c1", CardKind::Monitor)))
            .unwrap();
        let err = ls
            .reduce(CanvasEvent::Confidence {
                target_id: "c1".to_owned(),
                target_kind: "monitor".to_owned(),
                value: 0.9,
                basis: "ab".to_owned(), // too short
                contradicts: vec![],
                evidence_tier: EvidenceTier::High,
            })
            .unwrap_err();
        assert!(matches!(
            err,
            lightarchitects_lightspace::error::ReducerError::ConfidenceBasisTooShort(_)
        ));
    }

    #[test]
    fn materialize_sets_phase() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Materialize { phase: 2 })
            .unwrap();
        assert_eq!(ls.state.materialize_phase, Some(2));
    }

    #[test]
    fn drawer_file_inserted_and_detachable() {
        let file = DrawerFileData {
            id: "f1".to_owned(),
            mime_type: "text/plain".to_owned(),
            content_uri: "helix://docs/readme".to_owned(),
            size_bytes: 100,
            provenance: test_provenance(),
        };
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::DrawerFile(file))
            .unwrap();
        assert!(ls.state.drawer_files.contains_key("f1"));

        let ls = ls
            .reduce(CanvasEvent::DrawerEvent {
                file_id: "f1".to_owned(),
                action: DrawerFileAction::Detach,
                actor: Actor::Operator,
                new_content_uri: None,
            })
            .unwrap();
        assert!(!ls.state.drawer_files.contains_key("f1"));
    }

    #[test]
    fn update_append_pushes_to_array() {
        let ls = Lightspace::new(test_session())
            .reduce(CanvasEvent::Card({
                let mut c = test_card("c1", CardKind::Trace);
                c.content = serde_json::json!({"events": []});
                c
            }))
            .unwrap()
            .reduce(CanvasEvent::Update {
                card_id: "c1".to_owned(),
                seq: 1,
                mode: UpdateMode::Append,
                path: Some("/events".to_owned()),
                payload: serde_json::json!({"ts": 1}),
            })
            .unwrap();
        let events = ls.state.cards["c1"].content["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
    }
}
