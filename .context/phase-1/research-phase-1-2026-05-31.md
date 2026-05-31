# RESEARCH Phase 1 — webshell-hitl-bridge
Date: 2026-05-31
Scope: standard

## Key Findings

### 1. QuestionInput schema (AskUserQuestion 1:1 match)
Anthropic AskUserQuestion JSON shape:
```json
{
  "questions": [{
    "question": "...",
    "header": "...",
    "multiSelect": false,
    "options": [{"label": "...", "description": "..."}]
  }]
}
```
Rust mapping with schemars: `#[serde(rename_all = "camelCase")]` on `QuestionInput`
and `Question` structs. `multiSelect` bool defaults to false.
LA extension: optional `headless_policy` at top level.

### 2. WebEventV2 / WebEvent extension pattern
- `WebEvent` = inner enum in `types.rs` (exhaustive)
- `WebEventV2` = envelope struct in `envelope.rs` that wraps `WebEvent` via `from_event()`
- `topic_for()` in `envelope.rs` is exhaustive — adding new variants WILL cause compile error
  until handled. This is intentional (compile-time completeness guard).
- `severity_for()` should classify `QuestionPrompt` as `Severity::Warn` (requires operator action)
- The `all_variants_produce_v1_topics` test in `envelope.rs` uses each variant explicitly —
  must add cases for `QuestionPrompt(QuestionPromptEvent{...})` and `QuestionAnswered(...)`
- Topics: `v1.conductor.question.prompt` + `v1.conductor.question.answered`
  (conductor namespace = orchestration/HITL, consistent with `v1.conductor.permission.requested`)

### 3. Type placement (gateway vs webshell — separate binaries)
- Gateway: `QuestionInput` + `Question` + `QuestionOption` + `HeadlessPolicy` + `QuestionAnswer`
  in `lightarchitects-gateway/src/core_tools/question.rs`
- Webshell: Mirror types `QuestionItem` + `QuestionOptionItem` + `QuestionHeadlessPolicy`
  in `lightarchitects-webshell/src/events/types.rs` (used in `QuestionPromptEvent`)
- Wire contract: both sides use `#[serde(rename_all = "camelCase")]` — JSON is the interface
- No cross-crate dependency needed: gateway serializes → HTTP POST → webshell deserializes

### 4. QuestionRegistry type (webshell side)
- `dashmap` already in workspace + lightarchitects-webshell Cargo.toml ✅
- `uuid` already in workspace + lightarchitects-webshell Cargo.toml ✅
- Registry: `type QuestionRegistry = DashMap<Uuid, oneshot::Sender<QuestionAnswer>>`
- Metadata: `type QuestionMetadata = DashMap<Uuid, QuestionPending>`
- `QuestionPending { tool_use_id: Uuid, questions: Vec<QuestionItem>, headless_policy: Option<QuestionHeadlessPolicy>, inserted_at: DateTime<Utc> }`
- AppState adds: `question_registry: Arc<QuestionRegistry>` + `question_metadata: Arc<QuestionMetadata>`
- SECURITY: DashMap iteration+mutation deadlock pattern — always collect keys to Vec before mutating
  (per feedback memory: feedback_dashmap_iteration_deadlock)

### 5. rmcp dep (webshell)
Confirmed from dep-audit-phase-0: add to `lightarchitects-webshell/Cargo.toml`:
```toml
rmcp = { version = "=1.7.0", features = ["server", "macros", "transport-streamable-http-server", "transport-streamable-http-server-session", "schemars"] }
```

### 6. schemars dep (gateway)
Not in workspace. Add directly to `lightarchitects-gateway/Cargo.toml`:
```toml
schemars = { version = "0.8", features = ["derive"] }
```
Pattern: `#[derive(Deserialize, schemars::JsonSchema)]` — no `use schemars::JsonSchema` needed
when using the path-qualified form.

### 7. rmcp tool macro pattern (for Phase 2 reference)
```rust
#[derive(Deserialize, schemars::JsonSchema, Default)]
struct QuestionInput { ... }

#[tool_router(server_handler)]
impl QuestionMcpServer {
    #[tool(name = "question", description = "Present a structured question...")]
    async fn question(&self, Parameters(input): Parameters<QuestionInput>) -> String { ... }
}
```

### 8. AppState extension risk
AppState has many fields + `Clone`. Adding fields requires updating:
- Struct definition (server/mod.rs ~L103)
- `new()` constructor (~L362-493)
- `for_test()` constructor (~L631-647)
Both constructors must be updated to init the new fields.

## GATE0-TF1 Resolution
Plan body inline C3 places QuestionRegistry in gateway subgraph (incorrect).
Correct placement (from C3_component.mmd): webshell subgraph.
Fix in W1.6: update plan body C3 to match authoritative diagram.
