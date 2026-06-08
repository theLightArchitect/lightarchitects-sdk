# C4 — Code: Reducer::reduce() dispatch + helpers

```
Lightspace::reduce(state, event)
├── Clone state → next (O(n) snapshot on every event; acceptable for ≤50 cards)
├── Match event (EXHAUSTIVE — no wildcard arm; #![deny(non_exhaustive_omitted_patterns)])
│   ├── Card(card)
│   │   ├── Validate provenance.agent, provenance.source (non-empty)
│   │   ├── validate_provenance_source_scheme() → CWE-22 scheme ACL
│   │   ├── Check card.id not already in next.cards → E_CANVAS_CARD_ID_COLLISION
│   │   └── card.state = Attached; insert into IndexMap
│   ├── Update { card_id, seq, mode, path, payload }
│   │   ├── Check seq > per_card_seq[card_id] → E_CANVAS_UPDATE_SEQ_REGRESSION
│   │   ├── Check payload size ≤ 64KiB → E_UPDATE_PAYLOAD_TOO_LARGE  (CWE-770)
│   │   ├── tick::apply_update(content, mode, path, payload)
│   │   │   ├── Replace → *content = payload.clone()
│   │   │   ├── Append  → json_pointer_append(content, path, payload)
│   │   │   └── Patch   → json_patch::patch (RFC 6902)
│   │   ├── per_card_seq[card_id] = seq
│   │   └── gates::auto_reeval_gates_for_field(next, card_id, path)
│   │       └── For each gate whose (requires_card_id, requires_field) matches:
│   │           eval_gate_against_state(next, gate_card_id) → update gating_evaluations
│   ├── Lifecycle { card_id, transition, actor, ghost }
│   │   ├── tick::compute_target_state(from, transition) → Ok(to) | Err(IllegalTransition)
│   │   ├── tick::authorise_transition(from, to, actor)  → copilot may not Detach
│   │   ├── card.state = to
│   │   └── if to == Detached && ghost: tombstones.push(Tombstone::from(card))
│   ├── Gating { card_id, gate, satisfied, reason }
│   │   ├── Verify card_id in cards
│   │   ├── prev = gating_evaluations.get(card_id).map(|g| g.satisfied)
│   │   ├── gating_evaluations.insert(card_id, GateEvalResult)
│   │   └── Record state-edge for AYIN span ONLY IF prev != Some(satisfied)
│   │       (conditional — not every Gating event emits a span; only state changes)
│   ├── Graduate { card_id, file_id, content_uri, content_mime, retain_tombstone }
│   │   ├── Verify card.state == Attached
│   │   ├── validate_content_uri_scheme(content_uri) → CWE-22
│   │   └── Push to pending_graduations (applied at tick boundary)
│   ├── Materialize { phase }
│   │   └── next.materialize_phase = phase
│   ├── BranchLane { card_id, lanes, fork_span_id, committed_lane_id }
│   │   └── tick::update_branch_lane_content(card.content, lanes, ...)
│   ├── Confidence { target_id, target_kind, value, basis, contradicts, evidence_tier }
│   │   ├── basis.len() ≥ 5 (non-trivial) → E_CONFIDENCE_BASIS_MISSING
│   │   ├── value in 0.0..=1.0
│   │   ├── record_confidence(next, ...)
│   │   └── if contradicts non-empty:
│   │       contradictions::detect_cycle_or_depth(graph, target_id, contradicts)
│   │       if depth ≥ MAX_CONTRADICTION_DEPTH=3:
│   │           pending_resolutions.push(synthesize_resolution(...))
│   ├── ContradictionResolution { winner_target_id, loser_target_ids, seq, depth_reached,
│   │                            cycle_yielded, contributing_seqs }
│   │   ├── max_contrib = contributing_seqs.max(confidence_seq)
│   │   ├── seq > max_contrib → E_CANVAS_UPDATE_SEQ_REGRESSION (prevents stale resolution)
│   │   └── apply_resolution(next, winner, losers, depth_reached, cycle_yielded)
│   ├── DrawerFile(file)
│   │   ├── Validate provenance (non-empty agent + source)
│   │   ├── validate_content_uri_scheme(content_uri)
│   │   └── drawer_files.insert(file.id, file)
│   └── DrawerEvent { file_id, action, actor }
│       └── tick::apply_drawer_event(next, file_id, action, actor, new_content_uri)
├── next.snapshot_seq += 1  (monotonic; saturating_add)
└── self.assert_invariants(&next)?   → verify all 5 invariants hold
```

**Gate auto-re-evaluation** (SERAPH+CORSO R2 requirement, CWE-367 TOCTOU):
Whenever an Update event touches a field, all gates that `requires_field` on that card must be re-evaluated synchronously within the same `reduce()` call. This ensures the state is consistent when `assert_invariants()` runs.
