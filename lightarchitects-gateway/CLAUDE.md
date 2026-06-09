# CLAUDE.md — lightarchitects-gateway

MCP gateway binary — the single entry point Claude Code connects to. Stdio JSON-RPC server + Arena HTTP + Conductor task queue + Agentic Loop engine.

**Crate version**: 0.3.0 | **License**: proprietary (gateway-only; rest of workspace is MPL-2.0)

---
## Operating Modes

Three modes in one binary (`src/main.rs`):

| Mode | Invocation | Purpose |
|------|-----------|---------|
| **MCP** (default) | `lightshell` | Stdio JSON-RPC server — Claude Code's MCP endpoint |
| **Arena** | `lightshell serve [--agent <name>]` | HTTP API + scheduler + autonomous heartbeat agents at `:8742` |
| **Conductor** | `lightshell conductor <cmd>` | Autonomous task execution queue |
| **Platform** | `lightshell platform [--port 8080]` | Private REST API (`/v1/platform/*`) backed by local Neo4j |
| **Stream events** | `--stream-events` (with any mode) | NDJSON agent bridge for webshell copilot |

CLI subcommands: `soul`, `corso`, `eva`, `quantum`, `seraph`, `status`, `config`, `builds list|show`, `setup`, `webshell start|control|status`, `canon list|check`, `initialize`, `routes`.

`--version` / `-V` must print and exit BEFORE tracing setup (operational scripts parse the output).

---
## Source Architecture

```
src/
├── main.rs              # CLI arg dispatch → mode selection
├── lib.rs               # Public module declarations
├── server.rs            # MCP JSON-RPC loop, tool registry, dispatch
├── config.rs            # GatewayConfig typed schema + loader
├── error.rs             # GatewayError hierarchy
├── llm.rs               # Shared LLM client (Ollama, OpenAI-compat, Anthropic)
├── version.rs           # Build-time version metadata
├── span_context.rs      # GatewaySpanContext task-local + atomic disk write
├── governance.rs        # ScopeGovernor — trust and scope enforcement
├── enrichment.rs        # Real-time helix enrichment after SOUL writes
├── rubric.rs            # LASDLC C1-C8 effectiveness rubric
├── squad_comms.rs       # Squad Comms MCP actions — HTTP delegation to webshell
├── conversational.rs    # Pair-programmer REPL mode (build-in-a-box)
│
├── agent_stream/        # Interactive coding agent — NDJSON streaming + TTY REPL
│   ├── strategy.rs      # Strategy dispatch — resume/register
│   ├── protocol.rs      # NDJSON wire protocol
│   ├── endpoint_policy.rs
│   └── session_memory.rs
│
├── providers/           # LLM provider implementations
│   ├── mod.rs
│   ├── anthropic.rs     # Anthropic API provider
│   └── tool_executor.rs # GatewayToolExecutor — routes LLM tool_use to skills
│
├── core_tools/          # 30+ MCP tool implementations (tools/list → tools/call dispatch)
│   ├── mod.rs           # Tool registry — matches action names to handler functions
│   ├── read.rs, write.rs, edit.rs, glob.rs, search.rs  # File operations
│   ├── bash.rs          # Shell execution with allowlist policy
│   ├── meta.rs, discover.rs, preset.rs, initialize.rs   # Gateway meta-tools
│   ├── orchestrate.rs   # Multi-sibling orchestration
│   ├── ask_user.rs      # HITL prompt relay
│   ├── security.rs      # Security scanning tools
│   ├── canon_check.rs, canon_evaluate.rs  # Canon validation
│   └── ...
│
├── cli/                 # Sibling CLI passthrough (soul, corso, eva, quantum, seraph)
│   ├── mod.rs, launcher.rs, output.rs, status.rs
│   ├── builds.rs        # Build portfolio from SOUL vault
│   ├── skills.rs, skill_trust.rs  # Skill loading + SHA-256 trust ledger
│   ├── webshell.rs      # Web GUI control
│   ├── vault.rs         # Vault-as-git operations
│   └── setup.rs, init.rs, config_cmd.rs
│
├── handlers/            # In-process sibling handlers (feature-gated: inline-*)
│   ├── mod.rs, registry.rs
│   ├── corso.rs, eva.rs, soul.rs, quantum.rs, ayin.rs, laex.rs
│
├── arena/               # Autonomous multi-agent research platform
│   ├── mod.rs, routes.rs, scheduler.rs, heartbeat.rs
│   ├── agent.rs, supervisor.rs, curator.rs, grounding.rs
│   ├── mcp_pool.rs, llm.rs, backend.rs, rate_limit.rs
│   ├── alerting.rs, auth.rs, arena_config.rs, compat.rs
│   └── conversation_routine.rs
│
├── conductor/           # Autonomous task execution loop
│   ├── mod.rs, config.rs, queue.rs, executor.rs
│   ├── loop_driver.rs, guardrails.rs
│
├── http/                # Platform HTTP mode (private REST API + middleware)
│   ├── mod.rs, state.rs, circuit_breaker.rs, etag.rs
│   ├── routes/          # platform.rs, helix.rs, arch.rs, admin.rs
│   └── middleware/       # auth.rs, rate_limit.rs, ayin_trace.rs, identity_extractor.rs, version.rs
│
├── channels/            # External messaging (Discord webhooks, Telegram, etc.)
├── security/            # HMAC-SHA256 signing + verification for LASDLC hooks
├── spawner/             # Sibling subprocess spawner + MCP proxy (feature: spawner)
├── vault/               # Vault-as-git — pre-push validation, companion repo sync
│
└── tests/               # Integration tests
    ├── handler_dispatch_contract.rs   # Must run with LIGHTARCHITECTS_BIN env var
    ├── strategy*.rs
    └── ...
```

---
## Build

The gateway is **excluded from the workspace** (`Cargo.toml` `exclude` list) due to worktree lockfile collisions.

```bash
# Build directly:
cd lightarchitects-gateway && cargo build --release

# Run tests (gateway tests NOT covered by workspace-level cargo test):
cd lightarchitects-gateway && cargo test --features inline-all

# Run a specific integration test binary:
cargo test --test handler_dispatch_contract

# Check for cargo lock before building:
lsof target/.cargo-lock 2>/dev/null && echo "BLOCKED"
# Recovery: pkill -9 -f "cargo test" across all windows
```

**`current_exe()` trap**: In integration tests, `std::env::current_exe()` resolves to the test runner binary. Set `LIGHTARCHITECTS_BIN` env var to `target/release/lightshell` for any E2E test that spawns subprocesses.

**Codesign**: Manual binary copies require `codesign -s - ~/.lightarchitects/bin/lightshell` or macOS Gatekeeper will SIGKILL (exit 137) on first run. `make deploy` handles this automatically.

---
## Key Patterns

### Tool dispatch

`server.rs` registers a single MCP `tools/list` entry: the `tools` meta-tool. All 30+ actions are routed through `tools/call` with `{action, params}`. Individual `lightarchitects_*` tools still work but aren't advertised. `core_tools/mod.rs` matches action names to handler functions.

### MCP flow

```
main() → server::run(config)
  → stdin/stdout JSON-RPC loop
  → tools/list → tool_definitions()
  → tools/call → dispatch(action, params) → handler
  → handler returns Value or spawns sibling via spawner/
```

### Feature flags

| Feature | Effect |
|---------|--------|
| `spawner` (default) | Sibling subprocess spawning via `spawner/` module |
| `inline-all` | Compile all in-process handlers (tests, some deployments) |
| `inline-corso`, `inline-eva`, etc. | Individual handler gates |

When `inline-all` is enabled without `spawner`, all sibling calls go through `handlers/` in-process instead of spawning subprocesses.

### Operator-wins invariant

Per-turn, per-slug: `clear_operator_invocations()` resets at turn start. If the operator invokes a skill before the LLM responds, `GatewayToolExecutor` aborts conflicting `tool_use` with `ToolError::SupersededByOperatorAction`.

### bash_policy.rs allowlist

Explicit allowlist (cargo, git, ls, cat, grep, rg, jq, make, pnpm, npm). Unlisted commands → `NotPermitted`. Fail-closed.

---
## AYIN Span Instrumentation (Phase 1-5, shipped 2026-05-26; extended vibe-coding-loop, 2026-05-31)

Spans written via `GatewaySpanContext` (task-local) + `write_span_to_disk` (atomic `tmp→rename` + `F_FULLFSYNC`).

**Key files**:

| File | Purpose |
|------|---------|
| `src/span_context.rs` | `GatewaySpanContext`, `SPAN_CTX` task_local, `spawn_with_span_context`, `write_span_to_disk` |
| `src/server.rs` | `emit_tool_dispatch_span` — `gateway.tool.dispatch` span per MCP tool call |
| `src/llm.rs` | `llm.call` span with `parent_id` from `current_span_ctx()` |
| `src/http/middleware/ayin_trace.rs` | `platform.http.request` span per Arena HTTP request |
| `src/agent_stream/strategy.rs` | `gateway.session.start` span at strategy entry |
| `src/agent_stream/mod.rs` | `interactive.session` span per CLI invocation; `interactive.turn` span per LLM turn |

**Span name reference** (for AYIN dashboard queries):

| Span label | Emitted by | Metadata fields |
|---|---|---|
| `gateway.tool.dispatch` | `server.rs` | `tool`, `actor` |
| `llm.call` | `llm.rs` | `model`, `stop_reason`, `parent_id` |
| `platform.http.request` | `ayin_trace.rs` | `method`, `path`, `status` |
| `gateway.session.start` | `strategy.rs` | `strategy` |
| `interactive.session` | `agent_stream/mod.rs` | `provider`, `restored_turns` |
| `interactive.turn` | `agent_stream/mod.rs` | `turn_index`, `input_len`, `duration_ms` |

Query interactive turns: `curl -s http://127.0.0.1:3742/api/ironclaw | jq '.[] | select(.label=="interactive.turn")'`

**Rules**:
- Use `spawn_with_span_context(async move { ... })` NOT bare `tokio::spawn` for async span writes
- Context defaults to `GatewaySpanContext { session_id: None, parent_id: None }` outside a `with_span_context` scope
- Spans >64KB are silently dropped (eviction-attack mitigation)
- Trace files: `~/lightarchitects/soul/helix/ayin/traces/gateway/<YYYY-MM-DD>/`
- `.cargo/ci-denylist.sh` enforces `spawn_with_span_context` usage

---
## Environment Variables (CLI + Agent Stream)

| Variable | Default | Purpose |
|---|---|---|
| `LA_LLM` | `claude` | Provider selector: `anthropic` · `claude` · `ollama` · `litellm` |
| `LA_LITELLM_BASE_URL` | `http://localhost:4000` | LiteLLM proxy URL (read when `LA_LLM=litellm`) |
| `LA_LITELLM_API_KEY` | `la-local-dev` | LiteLLM bearer key — matches proxy `master_key` |
| `LA_LITELLM_MODEL` | `local-llama` | Model alias declared in `litellm.config.yaml#model_list` |
| `LIGHTARCHITECTS_BIN` | `lightarchitects` | Path to the gateway binary for E2E integration tests |

**Vibe-coding CLI workflow** (`LA_LLM=litellm`):
1. Start LiteLLM proxy: `litellm --config ~/.lightarchitects/litellm.config.yaml --port 4000`
2. Set env: `export LA_LLM=litellm LA_LITELLM_MODEL=<alias>`
3. Launch: `lightarchitects --interactive` (or from webshell provider drawer)
4. Ctrl-C exits cleanly within 100ms via `Arc<AtomicBool>` cancellation signal
5. Session history written to `~/lightarchitects/soul/helix/ayin/session/<session-id>.json`; next run restores it automatically via `HelixSessionMemory`

---
## Integration Tests

```bash
# All integration tests (with in-process handlers):
cd lightarchitects-gateway && cargo test --features inline-all

# Specific test binary:
cargo test --test handler_dispatch_contract

# E2E tests require binary path override:
LIGHTARCHITECTS_BIN=target/release/lightshell cargo test --test handler_dispatch_contract

# Single test by name (filters by test function name, not file name):
cargo test --test handler_dispatch_contract test_name_substring
```

Name filters against files with no matching function names silently report "N filtered out" with exit 0 — looks like success, isn't.
