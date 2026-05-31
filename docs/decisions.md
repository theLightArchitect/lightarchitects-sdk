# cockpit-wave-composer — Architecture Decisions

## Phase 1 — Architecture Verification (2026-05-30)

### Contract verification summary

All integration surfaces confirmed against live codebase HEAD (c606681):

| Surface | Status | Notes |
|---------|--------|-------|
| `spawn_autonomous_build(BridgeContext)` | ✓ CONFIRMED | `lightsquad_bridge.rs:110` |
| `validate_wave_ownership(&[Task])` | ✓ CONFIRMED | `wave_dispatcher.rs:144` — `pub fn` |
| `IndirectInjectionShield::detect()` | ✓ CONFIRMED | `agent/indirect_injection_shield.rs:179` |
| `AppState.event_tx` | ✓ CONFIRMED | `server/mod.rs:111` |
| `AppState.hitl_queue` | ✓ CONFIRMED | `server/mod.rs:243` |
| `AppState.mock_workers` | ✓ CONFIRMED | `server/mod.rs:263` |
| `CockpitPreset` (7 variants) | ✓ CONFIRMED | `lib/cockpit/stores.ts` — engineer/security/ops/quality/knowledge/researcher/testing |
| No `/api/cockpit/wave` route yet | ✓ CONFIRMED | Clean surface for Phase 2 |

### Drift finding: cardRoles count

**Finding**: Plan authored with `cardRoles 13→14` (wave-composer as 14th role). Actual current count is 14 (strategy-catalogue was added by commit `d44c7dc` on 2026-05-29). Wave-composer will be the 15th role.

**Correction**: All plan references updated 13→14 to 14→15 and "14th role" to "15th role".

**Affected plan sections**: §3.3 C3 diagram, §5 file-function-map, Phase 3 Wave 1 tasks, Phase 3 exit criteria, §13.1 pre-flight, §13.2 close-out.

### Implementation note: validate_wave_ownership pre-spawn

`validate_wave_ownership` takes `&[Task]` (not `&[TaskSpec]`). The Phase 2 handler's `build_task_specs()` produces `Vec<TaskSpec>`. For the pre-spawn ownership gate, the handler constructs minimal `Task` stubs (only `id` and `file_ownership` populated) for the validation call, then proceeds to `spawn_autonomous_build()`.

This is intentional: early rejection at the HTTP boundary (400 before any goroutine is spawned) is cheaper than letting `dispatch_wave` reject mid-build.

### Diagrams present

- §3.3.1 C2 Container: `C4Container` — confirmed present
- §3.3.2 C3 Component: `C4Component` — confirmed present  
- §3.3.3 Screen-flow: `graph LR` — confirmed present
- §3.3.4 State machine: `stateDiagram-v2` — confirmed present
