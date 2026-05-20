# Fleet JSONL Format Specification

> **Status**: Verified — ≥3 live records extracted from `~/.claude/projects/**/*.jsonl`  
> **Assumption A2 result**: CONFIRMED — field names match binary analysis from 2026-05-15  
> **Verification date**: 2026-05-18  

---

## Source

Claude Code writes one JSONL record per turn to `~/.claude/projects/{project-slug}/{session-id}.jsonl`. Each record is a JSON object on a single line. The `message.content` array may contain `tool_use` blocks; when `name == "Agent"`, the `input` object is the Agent invocation payload.

---

## Live Record Evidence

Extracted from three separate project sessions (field names confirmed consistent across Claude Code 2.x):

### Record Set A — lightarchitects-sdk project (5 records)

All five records share identical field shape:

```json
{
  "type": "tool_use",
  "id": "toolu_01ALqTjRgrdvD4GBQTNBbjVk",
  "name": "Agent",
  "input": {
    "description": "Sprint 4-A engineer: implement vital-streaming-prism (C2 streaming + C3 permission gating + frontend)",
    "subagent_type": "lightarchitects:engineer",
    "run_in_background": true,
    "isolation": "worktree",
    "prompt": "<REDACTED — never stored or transmitted>"
  }
}
```

### Record Set B — la-coord-test project (6 records)

Field shape (no `run_in_background`, no `isolation`):

```json
{
  "type": "tool_use",
  "id": "<toolu_...>",
  "name": "Agent",
  "input": {
    "description": "CORSO engineer lens — architecture review",
    "subagent_type": "lightarchitects:engineer",
    "prompt": "<REDACTED>"
  }
}
```

### Record Set C — Projects session (multiple records)

Mixed shapes — `run_in_background` present on some, absent on others:

```json
{
  "type": "tool_use",
  "id": "<toolu_...>",
  "name": "Agent",
  "input": {
    "description": "Explore SDK project structure",
    "subagent_type": "Explore",
    "run_in_background": true,
    "prompt": "<REDACTED>"
  }
}
```

---

## Field Specification

### Required fields (always present)

| Field | JSON type | Description |
|-------|-----------|-------------|
| `description` | `string` | Human-readable label for the agent task. Always present. May contain unicode. Verified present in all 20+ sampled records. |
| `prompt` | `string` | **SECURITY BOUNDARY — NEVER STORED OR TRANSMITTED.** Full instruction text sent to the sub-agent. Omitted from all FleetSpan fields by serde `skip` attribute. |

### Optional fields (conditionally present)

| Field | JSON type | Default when absent | Description |
|-------|-----------|--------------------|---------|
| `subagent_type` | `string` | `""` (treat as unknown) | Agent type identifier. Values observed: `"lightarchitects:engineer"`, `"lightarchitects:ops"`, `"lightarchitects:knowledge"`, `"lightarchitects:testing"`, `"lightarchitects:security"`, `"lightarchitects:quality"`, `"lightarchitects:researcher"`, `"Explore"`, `"feature-dev:code-reviewer"`, `"general-purpose"`. May be absent on older records. |
| `run_in_background` | `bool` | `false` | Whether the agent was spawned in background (tmux-pane mode). |
| `isolation` | `string` | `null` | Isolation mode. Observed value: `"worktree"`. Field absent on non-worktree spawns. |

### Fields observed but NOT in the allowlist

| Field | Observed? | Disposition |
|-------|-----------|-------------|
| `model` | Not seen in live records (absent from all 20+ samples) | Serde `#[serde(default)]` deserialization silently ignores it (SCR1-F1 forward-compat) |
| Any other field | Not observed | Forward-compat: unknown fields silently ignored via `#[serde(deny_unknown_fields)]` = NOT applied; use default serde behavior |

---

## FleetSpan Allowlist (data boundary)

Only these fields from the JSONL input MAY be stored in a `FleetSpan` or transmitted over any API surface:

```
{ description, subagent_type, run_in_background, isolation, tool_use_id }
```

`tool_use_id` comes from the outer `tool_use.id` field (e.g., `"toolu_01ALqTjRgrdvD4GBQTNBbjVk"`), NOT from the `input` object. It is used as `agent_id` in `FleetSpan`.

**`prompt` is EXCLUDED from the allowlist.** It must never appear in any `FleetSpan` field, any API response, or any SSE event payload.

---

## Tailer Record Shape

The JSONL tailer (`ClaudeJsonlTailer`) watches the session JSONL file and emits `TailerEvent` on each new line. The full outer record structure is:

```json
{
  "type": "user",
  "message": {
    "role": "assistant",
    "content": [
      {
        "type": "tool_use",
        "id": "toolu_...",
        "name": "Agent",
        "input": { ... }
      }
    ]
  },
  "uuid": "<record-uuid>",
  "timestamp": "..."
}
```

The tailer must navigate: `record.message.content[*]` where `item.type == "tool_use"` AND `item.name == "Agent"`.

---

## Forward Compatibility

Claude Code 2.x JSONL format has been stable across the records sampled. Per Assumption A2:

- **Field additions**: New fields in `input` are silently ignored (serde default behavior, no `deny_unknown_fields`).
- **Field removals**: `subagent_type`, `run_in_background`, `isolation` are already `Option<T>` in the deserializer — removal is handled.
- **`prompt` removal**: If Claude Code stops emitting `prompt`, the tailer continues working normally.
- **Risk**: Format change in a future Claude Code version. Mitigation: integration tests use fixture files from current format; CI catches breakage.

---

## Phase 1 Exit Criterion — Verification Summary

| Check | Result |
|-------|--------|
| ≥3 live Agent tool_use records extracted | PASS — 20+ records across 3 project sessions |
| `description` field present | CONFIRMED — all records |
| `subagent_type` field present | CONFIRMED — present in most; absent in some (correctly optional) |
| `run_in_background` field present | CONFIRMED — present when background spawn |
| `isolation` field present | CONFIRMED — present in worktree-isolation spawns (value: `"worktree"`) |
| `prompt` field present (must be excluded) | CONFIRMED — present in all records; NEVER stored |
| `model` field present | NOT OBSERVED in any live record |
| `tool_use_id` format | CONFIRMED — `"toolu_"` prefix, 27-char alphanumeric |
