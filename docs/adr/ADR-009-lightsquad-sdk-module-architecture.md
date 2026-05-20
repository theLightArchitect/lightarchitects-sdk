# ADR-009: lightsquad as SDK feature-gated module

**Status**: Accepted
**Date**: 2026-05-18
**Authors**: Kevin (architect), Claude (engineer)
**Supersedes**: Initial proposal of `delivery_arena/` under `lightarchitects-gateway` (reverted commit 48e6af3 via a7a8e7b)
**Related**: ADR-005 (ayin-feature-gated-wrapper) — same pattern, prior art in this workspace

---

## Context

The Ironclaw program (autonomous code-delivery orchestration engine per canonical architecture spec at `~/Downloads/ironclaw-architecture.pdf`) needs implementation in the Light Architects platform. The implementation must:

1. Reuse existing SDK primitives — `helix::HelixStore` (Neo4j-backed vault), `turnlog::TurnEntry` (HMAC-chained log), `squad_registry` (7-sibling routing), `agent::ClaudeCliProvider` (worker subprocess spawn), `crypto`, `auth`, `lasdlc`, `platform::PlatformClient`
2. Extend nearai/ironclaw upstream (Rust, Apache-2.0, pinned at `4fea8b3546` per `.audit/nearai-ironclaw-license-supply-chain-2026-05-18.md`) for gate/lease/safety primitives
3. Be consumable from `lightarchitects-webshell` and `lightarchitects-gateway` (and future LA tooling)
4. Keep business logic inside the SDK per current convention
5. Survive the eventual OSS split of the SDK (private business logic / public OSS consumable)

Three architectural patterns were evaluated:
- **Option 1**: Module in `lightarchitects` SDK behind `lightsquad` feature flag, consumers add `"lightsquad"` to their existing `features = [...]` list
- **Option 2**: Separate `lightsquad` workspace crate depending on `lightarchitects` + nearai git deps
- **Option 3**: Multi-crate from day 1 (lightsquad-core + lightsquad-engine + lightsquad-supervisor + ...)

## Decision

**Option 1 selected.** lightsquad is a feature-gated module of the existing `lightarchitects` SDK crate. Consumers enable it by adding `"lightsquad"` to the features list of their `lightarchitects` dependency.

```rust
// lightarchitects/src/lib.rs
#[cfg(feature = "lightsquad")]
pub mod lightsquad;
```

```toml
# lightarchitects/Cargo.toml
[features]
lightsquad = ["agent-cli", "http-client", "credentials"]

# lightarchitects-webshell/Cargo.toml, lightarchitects-gateway/Cargo.toml
[dependencies]
lightarchitects = { path = "...", features = [..., "lightsquad"] }
```

## Rationale

### Why Option 1 (workspace-convention match)

The `lightarchitects` SDK currently uses **26+ feature flags** as its decomposition mechanism (`sqlite`, `search`, `cypher`, `embedding-*`, `observe`, `neo4j`, `file`, `dual`, `ssh`, `cli`, `keychain`, `credentials`, `providers-anthropic`, `agent-cli`, etc.). The webshell consumes the SDK via:

```toml
lightarchitects = { path = "../lightarchitects", features = [
    "credentials", "providers-anthropic", "providers-openai",
    "credentials-display-names", "credentials-detailed-locator",
] }
```

Adding `"lightsquad"` as one more entry follows the established convention exactly. ADR-005 (ayin-feature-gated-wrapper) is direct prior art for the same pattern in this workspace.

### Why not Option 2 (separate crate)

Initially proposed by Claude based on abstract Rust patterns (tower / sqlx / axum multi-crate workspaces). User correctly identified this as inconsistent with the local workspace convention. The cross-examination revealed:

- Claim "industry best practice" relied on training-data pattern-matching, not verified local convention
- Citation "serde_json extracted from serde" was factually wrong (serde_json was always separate)
- Citation "tonic uses git deps for prost" was unverified (tonic actually uses crates.io deps)

Lesson logged: `~/.claude/projects/-Users-kft-Projects/memory/feedback_workspace_convention_over_abstract_best_practice.md` — established workspace conventions trump abstract industry best practice when consistency is a value.

### Why not Option 3 (multi-crate from day 1)

Premature. The lightsquad module begins with 9 sub-modules; multi-crate decomposition is justified at higher complexity. The extraction path is preserved (see Future Extraction below).

## SDK reuse mapping — no adapters

| lightsquad module | Reuses SDK module directly | nearai upstream extension (Phase 3+) |
|---|---|---|
| `wave_dispatcher` | `tokio` (via SDK transitively) | — |
| `merge_agent` | — | `git2` (add as optional dep in Phase 3) |
| `review_gate` | — | `ironclaw_engine::gate::ExecutionGate` trait + GatePipeline |
| `decision_pipeline` | `squad_registry::SquadRegistry`, `platform::PlatformClient`, `turnlog::TurnEntry` | — |
| `preflight` | `credentials` (Step 5 API-key verification) | — |
| `supervisor` | `turnlog::TurnLogWriter` (HMAC-chained ledger), `platform::PlatformClient` (canon resolution) | — |
| `worker_spawn` | `agent::ClaudeCliProvider` (already implements subprocess spawn + G1 `sanitize_params`) | — |
| `light_architects` | `squad_registry::SquadRegistry` | — |

**Key principle**: lightsquad imports SDK modules via their PUBLIC API surface. No reaching into internal modules. This discipline keeps the future-extraction path mechanical.

## Three subsection designs (Task #22 deliverable)

### 1. Helix primitive mapping

SDK exposes 5 helix primitives: `Helix`, `Step`, `Strand`, `HelixLink`, `SharedExperience`. Mapping to lightsquad concepts:

| Helix primitive | lightsquad concept | Rationale |
|---|---|---|
| `Helix` | Per-build container (Build = Helix) | Each build accumulates Steps as it progresses; the build is the helix |
| `Step` | Task execution outcome | Each task produces one Step: status, agent, duration, files touched, gate verdict |
| `Strand` | Wave (parallel tasks within a wave form a strand) | Strand = ordered collection of related Steps; wave is exactly that |
| `HelixLink` | Decision provenance edges | Decision → caused-by → prior-decision; LinkType already supports this |
| `SharedExperience` | Cross-build learnings | Patterns extracted at Phase 7 ENRICH that promote to helix for next-build context bundle |

Implementation in Phase 4: `supervisor.rs` uses `HelixStore::search()` for canon-grounded context retrieval; `decision_pipeline.rs` writes HelixLink edges for decision provenance; Phase 7 ENRICH writes SharedExperience records.

### 2. Turnlog NDJSON schema for Supervisor decision ledger

The canonical PDF spec says the Supervisor appends every decision to `/.ironclaw/decisions.md`. We replace plain-text Markdown with `turnlog::TurnEntry` records — gaining HMAC-chained tamper detection for free.

Per-decision fields (suggested mapping into TurnEntry's flexible payload):

```rust
// Conceptual schema (concrete fields land in Phase 4)
{
  "ts_ns": <i64>,               // turnlog provides
  "kind": "SupervisorDecision", // EntryKind variant (Phase 4: add to EntryKind enum)
  "build_id": "<codename>",
  "decision_id": "<uuid>",
  "task_id": "<task_id>",       // null if program-level
  "layer": "Canon" | "Northstar" | "LightArchitect" | "User",
  "rationale_summary": "<one sentence>",
  "input_hash": "<sha256 of decision input>",
  "lightarchitect_invoked": "security" | null,
  "sibling_dispatched": "SERAPH" | null,
  "model_used": "sonnet-4.6" | "haiku-4.5" | "qwen3-coder-480b",
  "latency_ms": <i64>,
  "cost_usd": <f64>,
  "outcome_link_id": "<helix_link_id>" // for HelixLink edge to prior decision
}
```

TurnLogWriter automatically HMAC-chains entries (no manual chain management). `is_helix_promotable()` lets Phase 7 ENRICH filter decisions worth promoting to helix as SharedExperience.

### 3. Squad_registry usage for 10 LightArchitect → 7 sibling routing

`squad_registry::SquadRegistry` loads `~/.lightarchitects/squad-registry.toml` at startup with one `SquadEntry` per sibling: `id`, `bin_path`, `helix_dir`, `mcp_args`.

Routing table (per `light_architects.rs` module docs):

```rust
// Conceptual (concrete impl in Phase 4)
pub fn dispatch(gate: GateDimension, registry: &SquadRegistry) -> SquadEntry {
    match gate {
        GateDimension::Architecture   => registry.get("corso"),    // primary
        GateDimension::Security       => registry.get("seraph"),
        GateDimension::Quality        => registry.get("corso"),
        GateDimension::Canon          => registry.get("laex"),
        GateDimension::Operations     => registry.get("eva"),      // primary
        GateDimension::Performance    => registry.get("eva"),      // co-owned with ayin
        GateDimension::Knowledge      => registry.get("soul"),
        GateDimension::Documentation  => registry.get("soul"),     // primary
        GateDimension::Testing        => registry.get("corso"),
        GateDimension::Research       => registry.get("quantum"),
    }
}
```

Fallback table for dimensions without sole-owner siblings:
- `[O] Operations` and `[P] Performance` co-owned by EVA + AYIN — route primarily to EVA, escalate to AYIN for trace-heavy diagnostics
- `[A] Architecture` and `[D] Documentation` — CORSO/SOUL primary; broadcast to secondary on conflict

10 LightArchitect personas → 7 distinct siblings means some siblings handle multiple gate dimensions. Acceptable: each dimension has a single primary owner, and `squad_registry` provides the indirection layer that lets us re-map without code changes.

## Consequences

### Positive
- Matches workspace convention (zero new patterns introduced)
- Zero new crates to maintain
- Webshell + gateway integration is one line each (`features = [..., "lightsquad"]`)
- SDK consumers who don't enable the feature pay zero compile cost for lightsquad
- HMAC-chained decision ledger via turnlog is **stronger** than canonical PDF's plain `decisions.md`
- HelixStore reuse means **no** vault adapter trait layer is needed (eliminates Task #22's original "SOUL vault adapter" scope)

### Negative / accepted tradeoffs
- The `lightarchitects` SDK accumulates one more responsibility (lightsquad orchestration alongside SDK clients + helix + turnlog + etc.)
- Single SDK release cadence — lightsquad versions can't evolve independently
- 27th feature flag in an already feature-heavy SDK (the embedding-* trio is already a noted extraction candidate per `project_lightarchitects_sdk_extraction_candidates.md`)

### Future extraction (when triggered)
Lightsquad is logged as an extraction candidate. Triggers:
- Sub-module count reaches ≥10 (currently 9)
- Independent publishing need (e.g., lightsquad goes OSS while SDK stays private)
- SDK compile times become painful for non-lightsquad consumers

Migration when triggered:
1. `cargo new --lib lightsquad` at workspace root
2. `mv lightarchitects/src/lightsquad/* lightsquad/src/`
3. Move optional deps from `lightarchitects/Cargo.toml` to `lightsquad/Cargo.toml` (make hard deps)
4. Remove the feature gate from lightarchitects; remove `lightsquad` from feature list
5. Consumers swap `features = [..., "lightsquad"]` → `lightsquad = { path = "../lightsquad" }`

Migration cost estimate: 2-4 hours assuming the "no internal cross-imports" discipline was followed (i.e., lightsquad only imported SDK modules via their public API surface).

## References

- Canonical Ironclaw spec: `~/Downloads/ironclaw-architecture.pdf` (15 pages, 15 sections)
- License audit: `.audit/nearai-ironclaw-license-supply-chain-2026-05-18.md`
- Workspace convention lesson: `~/.claude/projects/-Users-kft-Projects/memory/feedback_workspace_convention_over_abstract_best_practice.md`
- Extraction candidate tracking: `~/.claude/projects/-Users-kft-Projects/memory/project_lightarchitects_sdk_extraction_candidates.md`
- Prior art ADR (same pattern): `docs/adr/ADR-005-ayin-feature-gated-wrapper.md`
- Phase 1 scaffold commit: `973016c`
- Phase 1 revert (Option 1 pivot): `a7a8e7b`
