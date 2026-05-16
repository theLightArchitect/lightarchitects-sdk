---
phase: phase-2-research
build: plan-builder-copilot-bridge
evaluated: 2026-05-15
---

# Phase 2 Research Findings — R1–R8

## R1: Subprocess streaming verification

**Verdict: (a) confirmed** — confidence: 95%

Single-turn `claude --print --output-format stream-json --verbose -p "<prompt>"` with
`--session-id <pre-minted-uuid>` (Turn 1) carries the structured /PLAN-style prompt without
HITL block. The existing copilot module already implements this pattern (`spawn_copilot_turn` 
in `src/copilot/mod.rs:232`). Form-provided Northstar short-circuits AskUserQuestion by
inclusion verbatim in the prompt body.

Residual 5% risk: copilot may emit an AskUserQuestion-shaped output despite pre-flight
Northstar. Mitigation: parse stream for `{"type":"ask_user_question"}` shape; surface as
inline form modal in PLAN view.

**Baseline-establish sub-step**: DEFERRED to Phase 3.
Reason: the exact `spawn_plan_draft` prompt template is a Phase 3 deliverable; running a
proxy-prompt baseline now would give imprecise μ+2σ values. Baseline will be run at Phase 3
completion using the exact production prompt template. Phase 5 [P] escape hatch (one
prompt-template retry + accept new baseline) remains available per plan.

## R2: SSE streaming crate audit

**Verdict: REUSE** — confidence: 99%

`src/events/sse_handler.rs` uses `axum::response::sse::{Event, KeepAlive, Sse}` +
`futures_util::stream::unfold` + `tokio::sync::broadcast::Receiver`. This exact pattern
supports both new channels:
- `/api/events/global`: broadcast channel (1:N subscribers) — matches existing pattern
- `/api/builds/plan/draft`: mpsc channel (1:1 subprocess→subscriber) — same stream::unfold
  pattern, different channel type

Zero new dependencies.

## R3: Dependency audit (sonatype-guide)

**Verdict: SKIP** — confidence: 99%

No new crate dependencies introduced. All needed types (uuid, thiserror, serde, axum,
futures-util, tokio) are existing workspace deps. Sonatype-guide audit not required.

## R4: Frontend review_verdict parser stub

**Verdict: NEW TypeScript types in Phase 3** — confidence: 99%

No existing `PlanDraftEvent`, `ReviewVerdict`, or `PlanDraftRequest` TypeScript types in
`src/lib/types.ts` or `src/lib/api.ts`. No existing api.ts calls for `/builds/plan/draft`
or `/builds/plan/commit`.

Phase 3 deliverables:
- Add `PlanDraftEvent` discriminated union to `types.ts` (mirrors Rust enum, `type` field)
- Add `ReviewVerdict`, `PlanDraftRequest`, `PlanDraftResponseEnvelope`, `PlanCommitRequest`
- Add `api.draftPlan()`, `api.commitPlan()`, `api.subscribePlanStream()` to `api.ts`
- Gate Commit button on last `type === 'verdict_block'` event with
  `verdict.validation_status === 'VALIDATED'`

No inline YAML parsing needed on frontend — backend emits structured `VerdictBlock` events.

## R5: Threat model

**Verdict: LOGGED** — confidence: 99%

T1–T5 documented in plan (Part IV R5, line 344+). CF-F16 displacement annotation added.
Residual risk levels: T1=LOW, T2=LOW, T3=LOW, T4=LOW, T5=MEDIUM (cross-session event leak
pending Phase 4 redaction filter). T2 (prompt injection) and T5 explicitly accepted as
known residuals with Phase 4 [S] gate follow-up per plan.

## R6: GlobalEventsOverlay audit

**Verdict: (b) PARTIALLY WIRED** — confidence: 99%

`src/components/GlobalEventsOverlay.svelte` EXISTS (217 lines). Mounted at App root in
`app.svelte`. Has:
- ✅ `eventsOverlayOpen` store toggle (global)
- ✅ Overlay UI (unread badge, merged feed, EventStream sub-component)
- ✅ Context-aware title from route
- ❌ No `/api/events/global` SSE subscription
- ❌ No `EventFilter` query parameter support
- ❌ No `GlobalEventStore` Rust backend (Phase 3 new)

Phase 3 scope for R6: extend existing component to subscribe to `/api/events/global`;
add filter UI (sibling/severity/build_id selectors); wire `globalEventStore` Rust backend.
Existing component structure is correct scaffolding — additive changes only.

`Cmd+E` / `Ctrl+E` keybind: `eventsOverlayOpen` store exists; `app.svelte` already has
the overlay at root. Keybind registration needed in `hotkeyRegistry.ts` (Phase 3).

## R7: FloatingPanel primitive audit

**Verdict: NEW** — confidence: 99%

`FloatingPanel.svelte` does NOT exist in the codebase.

Design decision: CSS `resize: both` viable for initial implementation (requires
`overflow: auto/hidden/scroll`; well-supported in Chrome/Firefox/Safari). Native resize
handle in bottom-right corner. Custom drag-resize (mouse event handler) deferred to
Phase 4+ enhancement.

Phase 3 FloatingPanel contract:
- Props: `width: number`, `height: number`, `x: number`, `y: number`, `minimized: boolean`
- Emits: `close`, `minimize`
- CSS: `position: fixed; resize: both; overflow: hidden`
- localStorage: persist `{width, height, x, y}` per `id` prop on `beforeunload`
- `inert` attribute on closed/minimized (not just `display:none`) per a11y

## R8: CopilotDrawer audit

**Verdict: FloatingPanel WRAPPER approach** — confidence: 97%

`src/components/CopilotDrawer.svelte` EXISTS (1301 lines). Mounted at App root.

Session-state location: `mode: $state<'chat' | 'terminal'>('chat')` is component-local.
Since CopilotDrawer is mounted at App root (app.svelte line 508), the component instance
persists across screen navigation — Svelte never destroys it. RK15 (chat-state
preservation) is mitigated by construction. No store promotion needed.

Refactor strategy: add `positionMode: $state<'drawer' | 'overlay' | 'fullscreen'>('drawer')`
as a new component-local state. When `positionMode === 'drawer'` (default), use current
bottom-docked CSS container. When `positionMode === 'overlay'` or `'fullscreen'`, wrap the
content block in `<FloatingPanel>`. Additive — no removal of existing drawer logic.

Effort estimate: ~3–4h (FloatingPanel NEW ~2h + CopilotDrawer mode prop wire ~1–2h).

`drawerHeightPx` store already global — overlay mode should set it to 0 to avoid layout
compensation when the drawer is floating.
