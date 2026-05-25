# Screen Flow — Webshell UI: Autonomous Build session

> Canon XLI: Architect-authored design input. Phase 1 deliverable.

```mermaid
graph LR
  subgraph "Webshell Screens"
    HOME[HomeScreen\n/ Builds tab]
    FORM["AutonomousBuildStartForm\n(new build modal)\ncodename · tier · goal text\nmodel: qwen3-coder or deepseek-v3.1"]
    PANEL["AutonomousBuildsPanel\nActive builds list\nper-build: status chip + progress"]
    LIVE["WaveSlotGrid (live)\nWave N/M · Slot 1-7\nReal-time task names + status\nDecision ledger tail"]
    HITL["HitlEscalationModal\nDecision context\nOptions (radio)\nApprove/Reject buttons\nTraceparent in payload"]
    LEDGER["DecisionLedgerTail\nLive NDJSON stream\nper-entry: layer + verdict + rationale\nHMAC hash abbreviated"]
    DONE["Build Complete view\ntotal decisions · escalations\nmerge SHA · AYIN trace link"]
  end

  HOME -->|"New autonomous build"| FORM
  FORM -->|"POST /api/builds {mode:autonomous}"| PANEL
  PANEL -->|"Click active build row"| LIVE
  LIVE -->|"Slot status: hitl_pending"| HITL
  HITL -->|"POST /hitl-resolve approve"| LIVE
  HITL -->|"POST /hitl-resolve reject"| LIVE
  LIVE -->|"Build status: complete"| DONE
  LIVE -->|"Decision ledger icon"| LEDGER
  LEDGER -->|"Back"| LIVE
```

## State machine: AutonomousBuild client-side

```mermaid
stateDiagram-v2
  [*] --> idle
  idle --> submitted : POST /api/builds

  submitted --> dispatching : SSE event build_start
  dispatching --> executing : SSE event wave_start

  executing --> hitl_pending : SSE event hitl_escalation
  hitl_pending --> resolving : POST /hitl-resolve (approve/reject)
  resolving --> executing : SSE event wave_resume

  executing --> merging : SSE event wave_complete
  merging --> executing : SSE event next_wave_start
  merging --> complete : SSE event build_complete

  executing --> failed : SSE event build_failed
  hitl_pending --> failed : timeout (operator doesn't respond in N minutes)
  resolving --> failed : POST /hitl-resolve reject (terminal)
  complete --> [*]
  failed --> [*]
```

## Component list (Svelte 5, Phase 5 deliverables)

| Component | File | Description |
|-----------|------|-------------|
| `AutonomousBuildsPanel` | `screens/AutonomousBuildsPanel.svelte` | Root panel — build list + drill-down |
| `AutonomousBuildStartForm` | `components/AutonomousBuildStartForm.svelte` | New build modal |
| `WaveSlotGrid` | `components/WaveSlotGrid.svelte` | Live wave/slot progress grid |
| `HitlEscalationModal` | `components/HitlEscalationModal.svelte` | HITL escalation decision UI |
| `DecisionLedgerTail` | `components/DecisionLedgerTail.svelte` | Live NDJSON decision stream |

## SSE events consumed by UI

| Event type | Payload | UI action |
|-----------|---------|-----------|
| `build_start` | `{build_id, codename, wave_count}` | `submitted → dispatching` |
| `wave_start` | `{wave_n, task_ids}` | Update WaveSlotGrid |
| `task_progress` | `{task_id, slot, status}` | Update slot chip |
| `hitl_escalation` | `IronclawHitlEscalationEvent` | Mount HitlEscalationModal |
| `wave_resume` | `{wave_n}` | `hitl_pending → executing` |
| `wave_complete` | `{wave_n, merged_tasks}` | `executing → merging` |
| `decision_entry` | `DecisionEntry` (NDJSON) | Append to DecisionLedgerTail |
| `build_complete` | `{build_id, merge_sha}` | `merging → complete` |
| `build_failed` | `{build_id, reason}` | `* → failed` |
