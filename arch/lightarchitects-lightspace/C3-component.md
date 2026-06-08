# C3 — Component: Reducer internals + CanvasEvent variants

## CanvasEvent enum (11 variants — exhaustive match, no wildcard)

```
CanvasEvent
├── Card(Card)                                        → attach card; provenance required; scheme ACL
├── Lifecycle { card_id, transition, actor, attribution, ghost }
│                                                     → state machine (Proposed→Attached→Minimized→Detached)
│                                                       attribution field drives AYIN audit trail (SCRUM AYIN R2)
├── Update { card_id, seq, mode, path, payload }      → patch/replace/append; seq monotonic; ≤64KiB
├── Graduate { card_id, file_id, content_uri, ... }   → detach + drawer file; content_uri ACL
├── Materialize { phase, session_id, intent }          → phase ladder advancement
├── Gating { card_id, gate, satisfied, reason }        → precondition update; auto-re-eval
├── BranchLane { card_id, lanes, fork_span_id, committed_lane_id }
│                                                     → parallel exploration lanes; fork_span_id for W3C trace link
├── Confidence { target_id, target_kind, value, basis, contradicts, evidence_tier }
│                                                     → evidence tier; cycle detection at depth ≥ 3
├── ContradictionResolution { winner_target_id, loser_target_ids, seq,
│                             depth_reached, cycle_yielded, contributing_seqs }
│                                                     → seq-regression guard; apply_resolution(); marks loop halt
├── DrawerFile(DrawerFile)                            → file attach with provenance + content_uri ACL
└── DrawerEvent { file_id, action, actor }            → detach/update/replace drawer file
```

> **Exhaustion**: `#![deny(non_exhaustive_omitted_patterns)]` — adding a 12th variant is a compile-time error until handled.

## Module partitioning (Canon XXIII)

```
lightarchitects-lightspace/src/
├── lib.rs               pub re-exports
├── types.rs             CanvasEvent, CanvasState, Card, Provenance, Gating, ...
├── error.rs             ReducerError (no I/O variants)
├── snapshot.rs          Snapshot { seq, payload, integrity_hmac }
└── engine/
    ├── mod.rs
    ├── reducer.rs       pub trait Reducer; Lightspace struct glue
    ├── state.rs         CanvasState invariants + per_card_seq
    ├── gates.rs         auto_reeval_gates_for_field()
    ├── contradictions.rs detect_cycle_or_depth(); synthesize_resolution()
    └── tick.rs          apply_update(), compute_target_state(), authorise_transition()
```

## Reducer trait surface

```rust
pub trait Reducer: Send + Sync {
    fn reduce(&self, state: &CanvasState, event: &CanvasEvent) -> Result<CanvasState, ReducerError>;
    fn snapshot(&self, state: &CanvasState) -> Result<Snapshot, ReducerError>;
    fn restore(&self, snapshot: &Snapshot) -> Result<CanvasState, ReducerError>;
    fn invariants(&self) -> &[&'static str];
}
```

**Purity invariant**: `reduce()` is pure — no clock reads, no I/O, no randomness. `snapshot()` reads clock (permitted — it is the persistence boundary, not the reducer).

## CardState machine

```
Proposed ──Attach──→ Attached ──Minimize──→ Minimized
                         │                       │
                      Detach ←──────Restore──────┘
                         ↓
                    Detached (tombstone if ghost=true)
```

**Authorise invariant**: only `Actor::Operator` and `Actor::Engine` may `Detach`. `Actor::Copilot` may only `Propose` and `Attach`.
