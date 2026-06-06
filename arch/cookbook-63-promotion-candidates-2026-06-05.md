# Cookbook §63 Promotion Candidates — 2026-06-05

> **Status (2026-06-05 14:42 PT, post-LÆX)**:
> - **C1 → RATIFIED** as §63.P5 in `canon://builders-cookbook` v3.14.0 (CONDITIONALLY_RATIFIED conditions applied)
> - **C2 → REJECTED for canon** (placement_error: no canon home for ergonomics pattern); routed to memory `feedback_axum_path_filesystem_collision.md`
>
> **Source**: /REFLECT session 2026-06-05 (Path A + Path B Phase 1)  
> **Pipeline**: Canon XXXIX — completed (a) memory → (b) candidate → (c) 7-doc contradiction check → (d) LÆX verdict + Kevin stamp  
> **Auto-application**: FORBIDDEN per Canon XXXIX — this ratification cleared the full HITL pipeline

This doc surfaces two Rust patterns proven during Path A + Path B Phase 1 that warrant promotion to Cookbook §63 (Rust guidance). Neither auto-promotes — both need LÆX review for contradiction check against the 7 canon docs, then operator stamp.

---

## Candidate §63.PX — Tolerant serde for new Option fields on JSONL/IPC structs

**Proposed Cookbook section text:**

> ### §63.P? — Tolerant deserialization for new optional fields on producer-untrusted parse structs
>
> When adding a new `Option<T>` field to a struct deserialized from external/untrusted producers (JSONL session files, IPC frames, webhook payloads, sibling subprocess output), use `#[serde(default, deserialize_with = "tolerant_or_none")]` so a malformed instance of THAT field never fails the entire record:
>
> ```rust
> #[derive(Deserialize)]
> pub(crate) struct ParseTarget {
>     pub critical_field: Option<String>,
>     #[serde(default, deserialize_with = "deserialize_X_or_none")]
>     pub optional_new_field: Option<X>,
> }
>
> fn deserialize_X_or_none<'de, D>(d: D) -> Result<Option<X>, D::Error>
> where
>     D: serde::Deserializer<'de>,
> {
>     let v = Option::<serde_json::Value>::deserialize(d)?;
>     Ok(v.and_then(|raw| serde_json::from_value(raw).ok()))
> }
> ```
>
> **Why**: A bare `Option<X>` field, when malformed in the upstream JSON (wrong type, garbage payload), causes the entire `ParseTarget` deserialization to fail. Every other field is lost. For producer-untrusted shapes (the SCR1-F1 case), this loses critical records silently.
>
> **When to use the tolerant pattern**: defense-in-depth scenarios — fleet tracking, agent state, observability spans, anything where an upstream bug shouldn't silently drop records.
>
> **When NOT to use it**: strict-contract fields where the producer MUST be correct (schema-required fields in HTTP request bodies, etc.) — there, plain `Option<T>` is correct and the validation error is the intended outcome.
>
> **Test pattern**: write a malformed-field test BEFORE the implementation. Don't trust your mental model of serde's error propagation.

**Section placement**: Under §63 (Rust guidance), as a sibling to existing forward-compat / SCR1-F1 entries. Suggested numbering depends on current §63 state.

**Evidence (Path B Phase 1 / B1)**:
- File: `lightarchitects/src/fleet/jsonl.rs`
- Initial implementation: `pub wave_context: Option<WaveContextInput>` without tolerant deserializer
- Test that caught it: `wave_context_malformed_does_not_block_spawn`
- Failure mode: malformed `wave_context` block (string instead of object) caused entire `AgentToolInput` to fail deserialization, `agent_spawned` never fired, agent silently disappeared from fleet tracking
- Fix landed: `deserialize_wave_context_or_none` function at `jsonl.rs:103-113`

**Contradiction check requirements (Canon XXXIX step c)**:
- [ ] **Builders Cookbook** — no existing §63 entry for this pattern; check for contradictions with SCR1-F1 narrative
- [ ] **Platform Canon** — no expected contradiction (this is a Rust serde tactic, not a constitutional rule)
- [ ] **Agents Playbook** — no expected contradiction
- [ ] **Architects Blueprint** — no expected contradiction (implementation-layer)
- [ ] **Operators Manual** — no expected contradiction
- [ ] **Security Guardrails** — pattern HELPS security (defense-in-depth); no contradiction
- [ ] **Northstar** — no expected contradiction

**Confidence**: HIGH (verified through direct experience this session, with test evidence)

**Pairs with memory entry**: `[[feedback-tolerant-serde-jsonl-optional-fields]]` (will exist post-promotion as a backreference) — **NOTE**: not written as memory per operator instruction (promotion-candidate routing)

---

## Candidate §63.PY — Axum `Path` vs `std::path::Path` collision in route modules

**Proposed Cookbook section text:**

> ### §63.P? — Axum route modules: don't alias `Path` either way
>
> In Axum route modules that need filesystem operations, `axum::extract::Path` (the route extractor) and `std::path::Path` (filesystem path type) collide on bare imports. **Don't `as` alias either** — both names show up in clippy lints and rustdoc, and aliases hide intent.
>
> Pattern that works:
>
> ```rust
> use axum::extract::{Path, Query, State};  // handler convention — unaliased
> use std::path::PathBuf;                    // PathBuf alone is fine (no collision)
>
> async fn my_handler(
>     State(s): State<Arc<PlatformState>>,
>     Path(name): Path<String>,             // axum extractor, the handler convention
>     ...
> ) -> Result<Response, Response> { ... }
>
> // At any helper site needing &std::path::Path, FULLY QUALIFY:
> fn list_files(root: &std::path::Path) -> Vec<String> { ... }
> ```
>
> **Why**: The handler signature convention in the gateway uses unaliased `Path` (matches every other handler in `platform.rs`, `helix.rs`, `arch.rs`). Aliasing it would break the visual pattern that helps readers scan handlers. Aliasing `std::path::Path` (e.g. `Path as FsPath`) introduces a name that doesn't grep — clippy lint messages and rustdoc still say `Path`.
>
> Full-qualifying `std::path::Path` at helper sites is uglier than aliasing but maintains handler readability without introducing a fictitious name.

**Section placement**: Under §63 (Rust guidance), as a handler-conventions subsection. Adjacent to existing route-handler patterns.

**Evidence (Path A / Phase 5)**:
- File: `lightarchitects-gateway/src/http/routes/builds.rs`
- Initial attempt: `use std::path::{Path, PathBuf};` — clippy `ptr_arg` rejected `&PathBuf` in helpers
- Switching to `&Path` then collided with `axum::extract::Path` in handler signatures (error E0107: missing generics)
- Fix landed: `use std::path::PathBuf;` only at module top; helpers use `&std::path::Path` fully qualified

**Contradiction check requirements (Canon XXXIX step c)**:
- [ ] **Builders Cookbook** — check for existing import-style entries
- [ ] **Platform Canon** — no expected contradiction
- [ ] **Agents Playbook** — no expected contradiction
- [ ] **Architects Blueprint** — no expected contradiction
- [ ] **Operators Manual** — no expected contradiction
- [ ] **Security Guardrails** — no security implication
- [ ] **Northstar** — no expected contradiction

**Confidence**: HIGH (verified through compile errors + iterative fix)

**Pairs with memory entry**: `[[feedback-axum-path-filesystem-collision]]` — **NOTE**: not written as memory per operator instruction (promotion-candidate routing)

---

## Process notes

Per Canon XXXIX, the next steps (not auto-applied):

1. **Operator review** (Kevin) — sanity-check whether either belongs in §63 vs another section vs not in canon at all
2. **LÆX contradiction check** — verify against 7 canon docs, flag any conflicts
3. **LÆX drafts canonical section text** — may differ from my proposal; LÆX has final wording authority
4. **Operator stamp** — RATIFIED date + version bump (Cookbook v3.X.0)
5. **Cookbook CHANGELOG entry** — recording the §63.P? addition

Until ratification, this file is the source of truth for the candidates. Memory entries are NOT created for these (per operator instruction during /REFLECT 2026-06-05) — they live here pending LÆX review.

## Cross-references

- /REFLECT session output: this doc + 4 memory entries (`feedback_contract_ship_checklist`, `feedback_schema_first_contract_authoring`, `reference_validate_sh_symmetric_edge_sweep`, `feedback_decisions_doc_before_implementation`)
- Related canon: Cookbook §63 (current state — review for fit)
- Canon evolution doctrine: Canon XXXIX (Platform Canon)
