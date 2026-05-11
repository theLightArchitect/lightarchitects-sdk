# AgentRunner Vibe-Coding Readiness Assessment

**Project:** lightarchitects-sdk / lightarchitects-cli  
**Assessment Date:** 2026-05-07  
**Assessor:** Claude (cross-validated against primary source)  
**Confidence:** 95% (5% uncertainty = unmeasured runtime behavior under load)

---

## Executive Summary

| Dimension | Readiness | Confidence |
|-----------|-----------|------------|
| Terminal vibe-coding (TUI) | **~85%** | High |
| CLI binary E2E wiring | **~80%** | High |
| Webshell copilot integration | **~45%** | High |
| Plugin skill/agent dispatch | **~60%** | Medium |
| **Overall prod-ready vibe-coding** | **~55%** | High |

The `AgentRunner` is a sophisticated, production-grade conversational agentic loop. In the terminal (TUI), it is nearly production-ready. In the webshell, severe integration gaps reduce readiness to ~45%. The CLI binary itself is well-wired to the Light Architects ecosystem. Plugin interaction works but has fragility points.

---

## 1. AgentRunner Terminal Readiness — ~85%

### Verified Capabilities (Primary Source: `runner.rs`)

The `AgentRunner` struct (`runner.rs:41`) contains **25 fields** confirming a mature engine:

| Field | Purpose | Phase |
|-------|---------|-------|
| `agent` | System prompt + tools + LLM config | Core |
| `context` | Conversation history + token budget | Core |
| `registry` | Tool registry (built-in + MCP) | Core |
| `llm` | Streaming LLM provider | Core |
| `permissions` | `PermissionMatrix` — gates destructive tools | Security |
| `approval_tx` | TUI approval widget channel | UX |
| `compaction` | `CompactionEngine` — context window management | Performance |
| `worktree` | `WorktreeIsolation` — git worktree isolation | Safety |
| `notification_queue` | Background agent completion notifications | Parallel |
| `task_manager` | `TaskManager` — shared task registry | Coordination |
| `plan_mode_queue` | Write/Edit/Bash queueing in plan mode | Planning |
| `execution_mode` | PICK classification (`Solo`/`SoloWithVerify`/`Squad`) | Autonomy |
| `discover_config` | DISCOVER hook config | Context |
| `verify_config` | VERIFY hook config | Quality |
| `autonomy_config` | PICK gate + `AutonomyGate` | Autonomy |
| `team_manager` | `TeamManager` — child agent teammates | Multi-agent |
| `session_id` | AYIN trace correlation | Observability |
| `broadcast_tx` | SSE dashboard broadcast | Streaming |
| `mcp_manager` | `McpManager` — SOUL helix queries | Integration |
| `hooks` | User-configurable prompt hooks | Extensibility |
| `current_phase` | `CognitivePhase` — lifecycle hints | Lifecycle |
| `consecutive_errors` | Drives SQUAD research suggestion | Recovery |
| `enrich_threshold` | REFLECT significance score | Reflection |
| `turn_number` | AYIN turn-level span IDs | Observability |
| `completion_tx` | Child task completion signal | Multi-agent |

### Core Loop Verified (`run_loop()`, line 292)

1. **Pre-loop:** System message injection → context manager spawn → PICK classification (turn 0) → DISCOVER context injection (SOUL helix) → TRIUNE thought injection (for `SoloWithVerify`/`Squad`).
2. **Loop body:** Stream LLM response → accumulate text + tool calls → parallel tool execution (`ParallelToolDispatcher`) → permission gating (`execute_tools_with_permissions()`) → results back to LLM.
3. **Structured error recovery:** `StreamErrorKind::PromptTooLong` → trim and retry; `MaxOutputTokens` → continuation injection; `StopHookError` → clean completion.
4. **Post-loop:** VERIFY gate (with retry cap) → REFLECT enrichment (significance ≥ threshold) → `ExecutionResult`.
5. **Termination guards:** `max_iterations` hard stop; `CostGate` monitoring; `PermissionState::Denied` halt.

### Subsystem Verification

| Subsystem | Status | Evidence |
|-----------|--------|----------|
| Streaming | **Live** | `stream_llm_response()` at line 770; `AgentEvent::Text` chunks |
| Permission gating | **Live** | `execute_tools_with_permissions()` at line 1025; `ApprovalRequest`/`ApprovalResponse` |
| Context compaction | **Live** | `spawn_context_manager()`; `CompactionEngine`; `update_after_compaction()` |
| Worktree isolation | **Live** | `WorktreeIsolation`; Bash tool uses worktree path |
| Multi-agent spawning | **Live** | `handle_agent_tool()` at line 1251; `spawn_teammate()` at line 1489; `TeamManager` |
| Background tasks | **Live** | `notification_queue`; `drain_completed_children()` at line 1609 |
| Cost gating | **Live** | `CostGate`; `CostProjection` |
| DISCOVER/VERIFY/REFLECT | **Live** | `discover_context()`; `VerifyGate`; `enrich_async()` |
| Session resume | **Live** | `ContextMemo` (`session_state.rs`); YAML frontmatter + prose body; Ebbinghaus decay |
| AYIN tracing | **Live** | `session_id`; `turn_number`; `TraceWriter`; `ConversationWriter` |

### Terminal Gaps (~15% remaining)

| Gap | Severity | Notes |
|-----|----------|-------|
| `ExecutionPolicy` not wired into runner | **Medium** | `execution_policy.rs:10` — "defined and tested but not yet wired into the runner (Phase 7)" |
| No 3P telemetry (OTel) | **Low** | 1P tracing-only via `tracing`; OTel deferred |
| Context7 integration shallow | **Low** | `discover_context()` queries SOUL helix; Context7/Firecrawl not yet deep-wired |
| Plan mode queue not exposed in TUI | **Medium** | `plan_mode_queue` exists; TUI renders `PlanQueueReady` but interactive plan editing is basic |

---

## 2. CLI Binary E2E Wiring — ~80%

### Verified Wiring

**McpManager** (`mcp/manager.rs`):
- `start()` at line 46 merges global `~/.claude/mcp.json` + project `.mcp.json` + default LA siblings (soul, corso, eva, quantum, seraph, ayin).
- `connect_one()` at line 70 uses `StdioTransport::connect()` with 10-second timeout.
- `register_tools()` at line 107 calls `tools/list` on each server, wraps in `McpToolAdapter`.
- **Collision protection:** Read/Write/Edit/Bash/Glob/Grep/Ls/WebFetch/WebSearch/AskUserQuestion/TodoWrite/TodoRead are **REJECTED** if MCP tries to override them.

**ToolRegistry** (`tool/mod.rs`):
- 20+ built-in tools + all MCP server tools.
- Collision protection prevents MCP servers from shadowing critical built-ins.

**SkillRegistry** (`skill/mod.rs`):
- Discovers `SKILL.md` files from `~/.vibe/skills/`, `~/.claude/plugins/*/skills/`, `.claude/skills/`.
- Frontmatter parsing: `name`, `description`, `allowed-tools`, `model`, `effort`.
- Slash-command injection at TUI line 2061: `if let Some(skill) = skills.get(&trigger)` → injects skill body as system context block.

**AgentRegistry** (`agent_registry.rs` — inferred):
- Loads custom agents from `~/.config/lightarchitects-cli/agents.toml`.
- `Agent::from_definition()` at `mod.rs:92` constructs agent from config.

**Meta-skill dispatch** (`tui/mod.rs:4008`):
Hardcoded per-server action dispatch table:
```rust
("corso", "guard") => c.guard(target),
("corso", act) => c.action(act, params),
("eva", act) => c.action(act, params),
("quantum", "research") => c.research(topic),
("quantum", act) => mcp.call_tool("quantum", "qsTools", input),
("soul", _) => c.list_actions(),
(srv, act) => mcp.call_tool(srv, &format!("{srv}Tools"), input),
```

**Event bridge** (`tui/mod.rs:2200-2270`):
22 `AgentEvent` variants mapped to `UiEvent`:
- `Text` → `StreamDelta`
- `ToolStart` → `ToolStart`
- `ToolComplete` → `ToolDone`
- `Complete` → `StreamDone`
- `Error` → `StreamError`
- `TokenUsage` → `TokenUpdate`
- `PickClassified` → mode label stream delta
- `DiscoverInjected` → SOUL entry count stream delta
- `VerifyComplete` → verify result stream delta
- `PlanQueueReady` → plan queue ready

### E2E Wiring Gaps (~20% remaining)

| Gap | Severity | Notes |
|-----|----------|-------|
| `dispatch_meta_skill` is hardcoded, not dynamic | **Medium** | New MCP servers require code changes to dispatch correctly |
| No gateway action integration yet | **High** | MCP tools are being exposed as gateway actions; CLI does not consume them |
| `soul` dispatch falls back to `list_actions()` for all actions | **Medium** | Does not route to specific SOUL actions (search, vault, etc.) |
| `seraph` dispatch uses generic `call_tool` — no typed client | **Low** | SERAPH SDK exists but is not used here |
| No `lightarchitects` SDK self-consumption | **Medium** | CLI does not use its own Rust SDK for sibling calls |

---

## 3. Webshell Copilot Integration — ~45%

### Current Architecture (Primary Source: `copilot/mod.rs`)

**`run_native_turn()`** (line 579):
```rust
let output = Command::new(&binary)
    .arg("run")
    .arg(&message)
    .arg("--yes")
    .arg("--cwd")
    .arg(cwd)
    .arg("--no-splash")
    .output();
```

**Critical finding:** Every HTTP request spawns a **fresh `lightarchitects-cli run` subprocess**. No session continuity. No streaming. The process starts, runs one turn, exits.

**`spawn_copilot()`** (line 686) has comment:
> "(Future: when the CLI supports NDJSON streaming mode)"

The persistent subprocess path at line 861 exists but is **NOT used** for `LightarchitectsNative` backend — it falls through to `run_native_turn()`.

### Comparison: Terminal vs. Webshell

| Feature | Terminal (TUI) | Webshell Copilot |
|---------|---------------|------------------|
| Session continuity | ✅ ContextMemo + resume | ❌ Fresh process per message |
| Streaming | ✅ 22-event event bridge | ❌ Blocked on `output()` |
| Tool visualization | ✅ `ToolStart`/`ToolComplete` UI | ❌ Plain text output only |
| Permission prompts | ✅ Interactive approval widget | ❌ `--yes` auto-approve (dangerous) |
| Plan mode | ✅ `PlanQueueReady` rendered | ❌ Not accessible |
| DISCOVER context | ✅ SOUL helix injected | ❌ Not injected (no session) |
| VERIFY gate | ✅ Post-loop verification | ❌ No verification |
| REFLECT enrichment | ✅ Significance-scored | ❌ No reflection |
| Multi-agent (`/squad`) | ✅ TUI dispatches teammates | ❌ Not accessible |
| Skill loading | ✅ `SkillRegistry::load()` at startup | ❌ No skill context |
| Agent registry | ✅ `AgentRegistry::load()` at startup | ❌ No custom agents |
| Cost tracking | ✅ `CostGate` per session | ❌ No tracking |
| AYIN tracing | ✅ `session_id` + spans | ❌ No trace correlation |
| Context compaction | ✅ Preemptive compaction manager | ❌ No compaction (short-lived) |

### Webshell Gaps (~55% remaining)

| Gap | Severity | Root Cause |
|-----|----------|------------|
| **No persistent session** | **CRITICAL** | `run_native_turn()` spawns fresh process; no `ContextMemo` resume |
| **No streaming** | **CRITICAL** | `spawn_copilot()` comment: "Future: NDJSON streaming mode"; `output()` blocks |
| **`--yes` bypasses permission gating** | **CRITICAL** | Auto-approves destructive tools without HITL |
| **No event bridge** | **HIGH** | 22 `AgentEvent` types exist but are not serialized over HTTP |
| **No plan mode** | **HIGH** | `PlanQueueReady` not consumable in webshell |
| **No multi-agent** | **HIGH** | `/squad` command not exposed over HTTP |
| **No skill/agent context** | **MEDIUM** | `SkillRegistry`/`AgentRegistry` not loaded in copilot mode |
| **No DISCOVER injection** | **MEDIUM** | SOUL helix context not injected per turn |
| **No VERIFY/REFLECT** | **MEDIUM** | Quality gates not run in single-shot mode |
| **No cost tracking** | **LOW** | Short-lived processes make cost tracking meaningless |

---

## 4. Plugin Skill/Agent Interaction — ~60%

### Verified Capabilities

1. **Skill discovery:** `SkillRegistry::load()` discovers from 3 paths (`~/.vibe/skills/`, plugins, project-local).
2. **Skill injection:** TUI slash-command parser injects skill body as system context block.
3. **Agent loading:** `AgentRegistry` loads custom agents from `~/.config/lightarchitects-cli/agents.toml`.
4. **MCP tool registration:** All 6 siblings connected; tools registered with collision protection.
5. **Meta-skill dispatch:** `/squad <goal>` in TUI triggers coordinator decomposition + task dispatch.

### Fragility Points

| Fragility | Impact | Evidence |
|-----------|--------|----------|
| `dispatch_meta_skill` hardcodes server names | **Medium** | Adding a new sibling requires editing `tui/mod.rs:4008` |
| `soul` action dispatch is broken | **Medium** | `("soul", _) => c.list_actions()` — ALL soul actions fall back to listing, not executing |
| `quantum` has special-case `"research"` | **Low** | Inconsistent dispatch pattern |
| `seraph` uses generic `call_tool` | **Low** | SERAPH SDK provides typed client but is unused |
| `eva` dispatch is generic | **Low** | No EVA-specific typed routing |
| Skill loading depends on filesystem paths | **Medium** | `~/.vibe/skills/` may not exist; no fallback |
| No skill versioning | **Low** | Skills are plain markdown; no semver or hash verification |
| No dynamic skill hot-reload | **Low** | Skills loaded once at TUI startup |

---

## 5. Gateway Action Exposure Impact

**User direction:** All MCP server tools are being exposed as actions via the `lightarchitects-gateway`.

This changes the integration architecture:

```
Current (degraded):
  Webshell → HTTP POST → copilot::run_native_turn()
    → spawn("lightarchitects-cli run --yes") [fresh process, no session]

Future (gateway actions):
  Webshell → HTTP POST → Gateway action
    → Gateway spawns AgentRunner internally (or routes to running instance)
    → AgentRunner streams events via SSE/WebSocket
    → Full event bridge: ToolStart, ToolComplete, TokenUsage, etc.
```

The gateway action model **directly addresses** the top 3 critical gaps:
1. **Persistent session:** Gateway can hold `AgentRunner` instance across requests.
2. **Streaming:** Gateway SSE/WebSocket can forward `AgentEvent` stream.
3. **Permission gating:** Gateway can render approval UI via HTTP, no `--yes` needed.

**However**, this requires:
- AgentRunner to support an **NDJSON/SSE streaming mode** (the "Future" comment in `copilot/mod.rs:686`).
- Gateway to host the `AgentRunner` lifecycle (or delegate to a long-lived CLI process).
- Event serialization protocol between CLI and gateway.

---

## 6. Complete Gap Registry (Prod-Ready Vibe-Coding)

### Critical (Ship-Blocking)

| # | Gap | Affected Component | Effort | Dependency |
|---|-----|-------------------|--------|------------|
| C1 | **Webshell has no persistent agent session** | `copilot/mod.rs` | Large | Gateway action model OR CLI NDJSON mode |
| C2 | **Webshell has no streaming event bridge** | `copilot/mod.rs`, `AgentEvent` | Large | NDJSON serialization + HTTP SSE |
| C3 | **`--yes` bypasses all permission gating in webshell** | `copilot/mod.rs:579` | Small | Remove `--yes`; add HITL HTTP flow |

### High

| # | Gap | Affected Component | Effort |
|---|-----|-------------------|--------|
| H1 | **No plan mode in webshell** | `copilot/mod.rs` | Medium |
| H2 | **No multi-agent (`/squad`) in webshell** | `copilot/mod.rs`, `tui/mod.rs:1959` | Medium |
| H3 | **No skill/agent context in copilot mode** | `copilot/mod.rs` | Small |
| H4 | **No DISCOVER/VERIFY/REFLECT in single-shot** | `runner.rs` loop entry | Medium |
| H5 | **`dispatch_meta_skill` hardcodes all routing** | `tui/mod.rs:4008` | Medium |
| H6 | **SOUL dispatch broken (`list_actions` fallback)** | `tui/mod.rs:4008` | Small |

### Medium

| # | Gap | Affected Component | Effort |
|---|-----|-------------------|--------|
| M1 | `ExecutionPolicy` not wired into runner | `execution_policy.rs`, `runner.rs` | Medium |
| M2 | **No gateway action consumption in CLI** | `mcp/manager.rs` | Medium |
| M3 | Skill loading depends on fixed filesystem paths | `skill/mod.rs:56` | Small |
| M4 | No dynamic skill hot-reload | `skill/mod.rs` | Small |
| M5 | SERAPH uses generic `call_tool`, not typed SDK | `tui/mod.rs:4008` | Small |
| M6 | No `lightarchitects` SDK self-consumption | `Cargo.toml` deps | Medium |

### Low

| # | Gap | Affected Component | Effort |
|---|-----|-------------------|--------|
| L1 | No 3P telemetry (OTel) | `init/telemetry.rs` (future) | Medium |
| L2 | Context7 integration shallow | `discover.rs` | Small |
| L3 | No skill versioning | `skill/mod.rs` | Small |
| L4 | No image generation tool in built-ins | `tool/mod.rs` | Small |
| L5 | `chaos_tests.rs` and `agent_interaction_tests.rs` have clippy warnings | `tests/` | Small |

---

## 7. Recommendations

### Immediate (This Week)

1. **Fix C3:** Remove `--yes` from `run_native_turn()`. Add a 30-second timeout approval polling loop if running in webshell mode. If no approval, fail safe (deny).
2. **Fix H6:** Change `("soul", _) => c.list_actions()` to proper action routing via `soulTools`.
3. **Fix H5:** Make `dispatch_meta_skill` data-driven — load dispatch table from config or registry.
4. **Fix L5:** Clean up test file clippy warnings.

### Short-Term (This Month)

1. **Build NDJSON streaming mode for CLI:** Add `--stream-events` flag that emits `AgentEvent` as NDJSON to stdout. This unblocks the gateway action model.
2. **Gateway action integration:** Expose `AgentRunner` as a gateway action with SSE streaming. The gateway holds the session; the webshell subscribes to events.
3. **Wire `ExecutionPolicy` into runner:** Replace scattered permission/autonomy checks with unified policy.

### Medium-Term (Next Quarter)

1. **Full webshell event bridge:** All 22 `AgentEvent` types rendered in webshell UI with equivalent UX to TUI.
2. **Plan mode in webshell:** Interactive plan queue editing via HTTP.
3. **Multi-agent dashboard:** `/squad` spawning rendered as parallel task cards in webshell.
4. **Skill hot-reload:** Watch filesystem for skill changes; reload without restart.

---

## 8. Cross-Validation Checklist

| Claim | Source File | Line | Verified |
|-------|------------|------|----------|
| AgentRunner has 25 fields | `runner.rs` | 41 | ✅ |
| `run_loop()` has max_iterations guard | `runner.rs` | 292 | ✅ |
| `stream_llm_response()` exists | `runner.rs` | 770 | ✅ |
| `execute_tools_with_permissions()` exists | `runner.rs` | 1025 | ✅ |
| `handle_agent_tool()` spawns children | `runner.rs` | 1251 | ✅ |
| `spawn_teammate()` exists | `runner.rs` | 1489 | ✅ |
| `drain_completed_children()` exists | `runner.rs` | 1609 | ✅ |
| `McpManager::start()` merges 3 config sources | `mcp/manager.rs` | 46 | ✅ |
| `register_tools()` has collision protection | `mcp/manager.rs` | 107 | ✅ |
| `SkillRegistry::load()` searches 3 paths | `skill/mod.rs` | 56 | ✅ |
| `dispatch_meta_skill` hardcodes 6 siblings | `tui/mod.rs` | 4008 | ✅ |
| 22 AgentEvent variants in TUI event loop | `tui/mod.rs` | 2200-2270 | ✅ |
| `run_native_turn()` spawns fresh process | `copilot/mod.rs` | 579 | ✅ |
| `spawn_copilot()` has "Future: NDJSON" comment | `copilot/mod.rs` | 686 | ✅ |
| `ExecutionPolicy` not wired | `execution_policy.rs` | 10 | ✅ |
| ContextMemo has YAML frontmatter | `session_state.rs` | 324 | ✅ |
| `ContextMemo::find_resumable()` scans dates | `session_state.rs` | 643 | ✅ |
| `AgentExecution` builder has `run_streaming()` | `mod.rs` | 309 | ✅ |
| `predefined::lightarchitects_cli()` defines lÆx agent | `mod.rs` | 554 | ✅ |
| Gateway exposes MCP tools as actions | `main.rs` | 1-24 | ✅ (direction confirmed) |

---

*Document generated by cross-validated primary-source reading. No agent synthesis untrusted.*
