# WebEventV2 ↔ A2A §III Non-Contradiction Check

**Phase 0 / D0.3 — webshell-event-bus-redesign**
Date: 2026-05-20 · Confidence: 92%

## Executive verdict

**0 hard conflicts.** The two envelopes operate at **different layers** with non-overlapping responsibilities:

- A2A §III = **agent ↔ agent** JSONL (build-run audit trail, on disk + JSON-RPC).
- WebEventV2 = **gateway → browser/AYIN/SOUL** SSE distribution wrapper (operator visibility).

A2A fields absent from WebEventV2 are absent **by design** (not collision). Two soft naming recommendations follow.

## A2A §III canonical envelope (agents-playbook.md §3.2, L156-174)

```jsonl
{"v":1,"agent_id":"...","session_id":"...","build_codename":"...",
 "program_codename":"...","worktree_path":"...","branch":"...","commit_sha":"...",
 "phase":1,"wave":1,"task_id":"...","timestamp":"<ISO>","type":"<MessageType>","payload":{}}
```

Optional: `request_id`, `response_to`, `confidence`, `citations`, `verdict`, `error`, `uncertainty_reason` (§3.3, Canon XXXV).

## Proposed WebEventV2 envelope (plan Part 0, L70-80)

```rust
pub struct WebEventV2 {
    pub topic: String,                 // "build.<codename>.gate.phase-3.pass"
    pub ts: chrono::DateTime<Utc>,
    pub agent: AgentId,                // server-set; tamper-resistant
    pub build_id: Option<Uuid>,
    pub severity: Severity,            // info | warn | error
    pub payload: serde_json::Value,
    pub legacy_type: Option<String>,   // back-compat with WebEvent::type
}
```

## Field-by-field mapping

| A2A §III field | WebEventV2 | Mapping rule | Conflict? |
|---|---|---|---|
| `v` | (implicit in `v1.*` topic prefix) | A2A `v=1` → topic namespace `v1.` reserved | None |
| `agent_id` | `agent: AgentId` | semantic rename, type-tightened | **Soft: rename WV2 → `agent_id`** |
| `session_id` | (omitted) | SSE consumer doesn't pin to build-run session | OK |
| `build_codename` | encoded in `topic` (`build.<codename>.*`) + `build_id` denorm | topic segment + UUID index | None |
| `program_codename` | encoded in `topic` if multi-build (`program.<x>.build.<y>.*`) | deferred topic grammar | None |
| `worktree_path` / `branch` / `commit_sha` | (in `payload`) | not browser/AYIN relevant | OK |
| `phase` / `wave` / `task_id` | encoded in `topic` segments | dot-path carries these | None |
| `timestamp` | `ts: DateTime<Utc>` | semantic match | **Soft: rename WV2 → `timestamp`** |
| `type` | `legacy_type` **+** new `topic` | dual-carry during transition | None — explicit back-compat |
| `payload` | `payload: serde_json::Value` | direct match | None |
| `request_id` / `response_to` / `verdict` / `confidence` / `citations` | (in `payload`) | one-way SSE; Canon XXXV stays in payload | OK |
| (NEW) | `severity: Severity` | additive UI-routing classification | None — additive |

## Current WebEvent types in webshell-ui (types.ts L282-314)

24 `EventType` `snake_case` variants — matches the 24-variant ratchet in `lightarchitects-webshell/tests/canon_doc_integrity.rs:24`. **No variant breaks** under WebEventV2: existing `type` string migrates 1:1 into `legacy_type`.

## Current gateway/webshell emit_event callsites

- `pub enum WebEvent` lives at `lightarchitects-webshell/src/events/types.rs:19` (NOT in gateway crate).
- **119** `WebEvent::*` references in `lightarchitects-webshell/src/` (excluding tests).
- Transport: `tokio::sync::broadcast::Sender<WebEvent>` per `BuildSession.event_tx` (`session.rs:68`).
- Sample sites: `copilot/mod.rs:360,379,508,529,554,590,786,1031`; `copilot/routes.rs:233,249,266`; `real_data.rs:663,694,707,723,764`; `memory/promoter_bridge.rs:170`; `memory/convergence.rs:163`.
- Migration cost: add `topic` computation at each call site (~1 line each); mechanical codemod, single Phase 1 wave.

## Identified conflicts

**None of HIGH severity.** Two LOW-severity friction points:

1. **`ts` vs `timestamp`** — naming asymmetry; zero functional impact, non-zero cognitive cost. **Recommend rename `ts` → `timestamp`** (match canon).
2. **`agent` vs `agent_id`** — A2A uses string field name; WebEventV2 uses typed value. **Recommend rename `agent` → `agent_id`** (wire serialization unchanged).

## Backwards-compat strategy

- `WebEvent::type` discriminant preserved in `WebEventV2.legacy_type` (plan L78).
- Per-route SSE views unchanged — consumers receive `legacy_type`-keyed payloads unless they explicitly call `subscribeByTopic` (plan L104, L376-385).
- A2A JSONL on disk remains canonical for agent ↔ agent; WebEventV2 is the distribution wrapper. **Two layers, no contention.**
- 24-variant ratchet test continues; add parity test that every variant produces a valid topic string (Phase 1 D1.3).

## Recommendation

**Proceed with WebEventV2 as proposed, with two pre-Phase-1 naming alignments:**

1. `ts` → `timestamp` (match A2A §3.2).
2. `agent` → `agent_id` (match A2A §3.2).

No A2A field requires rename. The envelopes solve different problems at different layers — exactly the LÆX+FE convergent finding that justified this build over `humming-publishing-bee`. Phase 0 Gate-0 [R][C] criterion "Non-contradiction check returns 0 conflicts" is **met** (0 hard; 2 soft naming nits captured for Phase 1 D1.1).
