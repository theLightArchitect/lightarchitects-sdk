# QA — agent_stream module (NDJSON + TTY dual-mode agent)

## Scope

`lightarchitects-gateway/src/agent_stream/` — the module that makes `lightarchitects` work like `claude` (TTY REPL) and like a streaming agent subprocess (NDJSON bridge for the webshell).

---

## 1. Architecture correctness

### Q: Does the module introduce any new external dependencies?
**A:** No. It reuses existing gateway infrastructure:
- `crate::arena::llm::LlmClient` — already used by Arena agent loop
- `crate::core_tools` — bash, read, write, edit, search, glob
- `tokio::io` — already a workspace dependency
- `serde_json` — already a workspace dependency

### Q: How does the agent loop terminate?
**A:** Five terminal paths, all emitting `AgentEvent::Complete`:
1. `### FINAL_OUTPUT` parsed from LLM response → `TerminationReason::Complete`
2. `MAX_ITERATIONS` (10) reached → `TerminationReason::MaxIterations`
3. `tokio::time::timeout(LLM_TIMEOUT)` fires → `TerminationReason::Timeout`
4. `ControlMessage::Interrupt` sets atomic flag → `TerminationReason::UserCancelled`
5. LLM returns `Err` → `TerminationReason::Error { message }`

### Q: What prevents the child from OOM-ing the webshell with unbounded stdout?
**A:** `MAX_NDJSON_LINE = 1 MiB` in `bridge.rs` (line 36). Lines exceeding this are truncated and discarded. The runner side has no equivalent bound because it is the *producer*; the consumer (bridge) carries the defense.

### Q: Why is `AgentRunner::new` synchronous (not `async`)?
**A:** It only performs synchronous work: `LlmClient::from_env()` (reads env vars), `GatewayConfig::default()`, and `std::env::var("HOME")`. No I/O. The original `async` was clippy-pedantic flagged as `unused_async`.

### Q: What LLM backends are supported?
**A:** Three, all via `LlmClient`:
- **Ollama** — local, `OLLAMA_HOST` + `OLLAMA_MODEL`
- **OpenAI-compatible** — `OPENAI_API_KEY` + custom `base_url`
- **Anthropic** — `ANTHROPIC_API_KEY`, native Messages API (`/v1/messages`), model defaults to `claude-sonnet-4-6`

The Anthropic backend was added specifically for this module so the default launcher model actually works.

---

## 2. Security surface

### Q: What tools can the agent call?
**A:** Six, hardcoded in `ALLOWED_TOOLS` (runner.rs:555):
`bash`, `read`, `write`, `edit`, `search`, `glob`.
Any other tool name returns `GatewayError::UnknownTool` and surfaces as a failed `ToolComplete`.

### Q: Can the agent escape the working directory?
**A:** The `bash` tool receives `cwd` injection (runner.rs:570–572), defaulting to the session's `cwd`. The `read`/`write`/`edit`/`search`/`glob` tools rely on `GatewayConfig.allowed_directories`, which includes the user's `HOME` (runner.rs:98–99). The existing `core_tools` implementations enforce directory boundaries.

### Q: Is the API key exposed to the child process?
**A:** No. The bridge (`bridge.rs:74–77`) intentionally does **not** inject `ANTHROPIC_API_KEY` into the child's environment. The CLI resolves its own credentials (keychain / config file) so a compromised agent process cannot exfiltrate the key via `/proc/self/environ`.

### Q: What env vars does the bridge pass to the child?
**A:** Whitelist-only (bridge.rs:87–95):
- `HOME`, `USER`, `SHELL`, `RUST_LOG`
- Any key starting with `LA_` or `LIGHTARCHITECTS_`
All other env vars are cleared via `.env_clear()`.

---

## 3. Protocol fidelity

### Q: Does the gateway-side protocol match the webshell-side protocol?
**A:** Yes, by design. `protocol.rs` on both sides uses identical `AgentEvent` and `ControlMessage` definitions with the same `serde(tag = "type" / "action", rename_all = "snake_case")` attributes. The doc comment in `protocol.rs` explicitly states: "Kept in-sync manually; both sides must agree on variant names and fields."

### Q: What happens if the NDJSON line is unparseable?
**A:** `process_line` in `bridge.rs` falls back to emitting it as `AgentEvent::Text` (line 224–228). This means garbage or non-JSON debug output from the CLI still streams to the browser rather than crashing the bridge.

### Q: How does the bridge know the turn is finished?
**A:** It tracks `saw_complete: bool`. Any `AgentEvent::Complete` sets it (bridge.rs:219–221). When stdout closes, if `saw_complete` is false, the bridge injects an `Error` + `Complete` pair so the browser UI doesn't hang (bridge.rs:151–157).

---

## 4. Error handling

### Q: What happens if `LlmClient::from_env()` fails?
**A:** `run_ndjson` / `run_interactive` return `Err` immediately with a message like "LLM client init failed: ANTHROPIC_API_KEY not set". The caller (`main.rs:56–58`) prints to stderr and exits 1.

### Q: What happens if a tool call times out?
**A:** `bash` has its own timeout parameter. LLM generation has `LLM_TIMEOUT = 180s`. On timeout, the runner emits `Error { message: "LLM timeout" }` + `Complete { Timeout }`.

### Q: What happens on `ControlMessage::Ping`?
**A:** The NDJSON loop silently skips it (runner.rs:155–157) — no event emitted, no turn started. This is correct: ping is a keepalive for the WebSocket transport layer, not an agent instruction.

---

## 5. Integration points

### Q: How does `main.rs` decide between TTY and NDJSON mode?
**A:** Priority order (main.rs:49–113):
1. `--stream-events` flag present → NDJSON mode (always, even if TTY)
2. No args + `stdin().is_terminal()` → TTY mode
3. Otherwise → falls through to existing MCP / CLI / Arena dispatch

### Q: What is the fallback if the user has `always_webshell: true` in launcher config?
**A:** The TTY path spawns `lightarchitects webshell start` as a child process (main.rs:74–102). This preserves existing behavior for users who configured the GUI launcher.

### Q: What binary does the webshell bridge spawn?
**A:** `resolve_binary("lightarchitects")` — changed from `"lightarchitects-cli"` because the CLI was merged into the gateway binary. The bridge calls `lightarchitects --stream-events --cwd <path>`.

---

## 6. Performance & resource limits

| Limit | Value | Location |
|-------|-------|----------|
| Max iterations per turn | 10 | `runner.rs:24` |
| LLM generation timeout | 180s | `runner.rs:27` |
| Max NDJSON line length | 1 MiB | `bridge.rs:36` |
| Stdout read chunk size | 8 KiB | `bridge.rs:140` |
| Bash tool timeout | caller-provided (default 120s) | `TOOL_DESCRIPTIONS` |

---

## 7. Checklist

- [x] Clippy clean (`cargo clippy -- -D warnings`)
- [x] Unit tests pass (`cargo test --lib` — 333/333)
- [x] No new dependencies
- [x] Protocol enums match webshell side
- [x] Security: env whitelist, no API key injection, directory boundaries
- [x] Graceful termination: all five paths emit `Complete`
- [x] Bridge fallback on parse failure (Text event, not crash)
- [x] TTY vs NDJSON shared core (`run_turn`)
