# CLAUDE.md — lightarchitects-webshell

Local web GUI shell. PTY-hosted agent session + 3D session-helix panel.

## Backends

| Backend | Env var | Binary dep | Auth |
|---------|---------|-----------|------|
| Claude Code (default) | — | `lightarchitects` on `$PATH` | Anthropic API key via Keychain |
| Mistral Vibe | `MISTRAL_API_KEY` | `vibe` + `vibe-acp` on `$PATH` | Mistral API key |

### MistralVibe (`MISTRAL_API_KEY`)

Stored as `secrecy::SecretString` — never as `String`. The key is zeroized on drop.

- `copilot::resolve_mistral_api_key()` → `Option<SecretString>` (env var, then macOS Keychain)
- `spawn_bridge()` injects via `cmd.env("MISTRAL_API_KEY", key.expose_secret())` **only** when `is_vibe == true`; non-vibe sessions get `env_clear()` + whitelist with no key re-injection (CWE-306 prevention)
- Spawn errors logged via `tracing::warn!`; the `AgentEvent::Error` payload is an opaque code (`"vibe_spawn_failed"`) — never the raw error string (CWE-209 prevention)

### Env whitelist (`spawn_bridge`)

After `env_clear()`, only these vars are forwarded: `PATH`, `HOME`, `USER`, `SHELL`, `RUST_LOG`, `LLM_BACKEND`, `OLLAMA_BASE_URL`, `OLLAMA_MODEL`, all `LA_*` / `LIGHTARCHITECTS_*` vars, and (vibe only) `MISTRAL_API_KEY`.

## MCP Host Proxy

The webshell can proxy requests to any MCP stdio server declared in `~/.lightarchitects/webshell-mcp.json`. Three routes are registered automatically:

```
GET  /api/mcp/servers  — list all managed servers + live state
GET  /api/mcp/tools    — list all cached tools across ready servers
POST /api/mcp/invoke   — { server, tool, input } → tool output
```

All routes require `AuthGuard`. Returns `503` when `webshell-mcp.json` is absent.

`AppState.mcp_host` is a `McpHostHandle = Arc<RwLock<Option<HostManager>>>` initialized
asynchronously via `tokio::spawn` in `AppState::new()` — webshell startup is non-blocking.

**Trust model**: 5-layer security (env isolation → sandbox-exec → process group → ScopeGovernor+SchemaValidator → TOCTOU-safe check). See `lightarchitects-webshell-mcp-host/README.md` and `docs/trust-model.md`.

**Tools UI**: Panel 5 in `src/screens/Tools.svelte` — server-filter dropdown, tool card grid, `McpToolForm` modal, result panel. Form generation from JSON Schema via `mcp-schema.ts` + `JsonSchemaField.svelte`.

**Config template**: `assets/webshell-mcp.json.default` — copy to `~/.lightarchitects/webshell-mcp.json` and update paths. Day-1: 6 siblings + @drawio/mcp + 1 reserve slot.

## Credential Substrate (§2.38 — cli-oauth-multi-provider, 2026-05-22)

6-provider credential management. All persistence via `/usr/bin/security` only (OA-3).

| Provider | Flow | Keychain service |
|----------|------|-----------------|
| Anthropic | ApiKey | `la-anthropic-credential` |
| OpenAI | ApiKey | `la-openai-credential` |
| Mistral | ApiKey | `la-mistral-credential` |
| GitHub | RFC 8628 Device Flow | `la-github-credential` |
| Ollama | CLI subprocess (`ollama list`) | `la-ollama-credential` |
| Google | OAuth 2.0 PKCE + RFC 8252 loopback | `la-google-credential` |

**8 routes (all require `AuthGuard` except the OAuth callback):**

```
POST   /api/auth/credential/google/init
GET    /api/auth/credential/google/callback     # browser redirect — no AuthGuard
POST   /api/auth/credential/github/device
POST   /api/auth/credential/github/poll
POST   /api/auth/credential/ollama/connect
POST   /api/auth/credential/{provider}/key      # body limit: 2 KB (transport) + 1 KB (handler)
GET    /api/auth/credential/{provider}/status
DELETE /api/auth/credential/{provider}
```

**Security properties enforced:**

- OA-1: PKCE 256-bit CSPRNG, SHA256 challenge (Google only)
- OA-2: OAuth state UUID validated against `AppState::oauth_states` DashMap; 120 s TTL with background eviction (60 s interval)
- OA-3: All credential I/O via `/usr/bin/security` argv only — no keyring, no log leakage
- OA-5: `redirect_uri` locked to `127.0.0.1:{port}` from server config — not user-controllable
- OA-7/8/10: Tokens and API keys never written to tracing spans
- OA-9: GitHub Device Flow `interval` ≥ 5 s enforced in route handler
- OA-12: All 6 service names are distinct compile-time constants (proptest + unit-verified)
- F7: OAuth error callback HTML-escapes `params.error` before page interpolation (XSS)
- F8: All keychain + subprocess calls run via `tokio::task::spawn_blocking`
- F10: `POST /{provider}/key` — `DefaultBodyLimit::max(2 KB)` at transport layer; `MAX_API_KEY_BYTES = 1024` in handler

**`AppState` fields added:**

```rust
pub oauth_states: Arc<DashMap<Uuid, OAuthPendingState>>   // TTL-evicted every 60 s
pub credential_store: Arc<DashMap<String, CredentialState>> // in-memory cache
```

**Worktree spec-file trap**: `~/lightarchitects/soul/helix/user/standards` symlinks to primary worktree. Always edit `~/lightarchitects/worktrees/<codename>/standards/canon/webshell-api-surface-v1.*` — never via the helix path during a feature build.

## LightSquad Autonomous Build — Env Vars

| Variable | Default | Purpose |
|----------|---------|---------|
| `LIGHTARCHITECTS_WEBSHELL_TOKEN` | *(generated at startup)* | Auth token for the webshell HTTP API. Set to a known value to make `curl` tests predictable: `LIGHTARCHITECTS_WEBSHELL_TOKEN=test-token ./target/release/lightarchitects-webshell` |
| `LIGHTSQUAD_MOCK_WORKERS` | *(unset = real LLM)* | When set (any value), skip real Ollama calls. Workers write `<task_id>.txt` + commit. Use for orchestration tests that don't need LLM output. |
| `LIGHTSQUAD_CODING_MODEL` | *(unset)* | Override the coding model and bypass `CLOUD_MODEL_REGISTRY` validation. Accepts any model name local Ollama knows (e.g. `llama3.2:3b`). Takes priority over `OLLAMA_MODEL`. |
| `OLLAMA_MODEL` | *(unset)* | Ambient model fallback. If `LIGHTSQUAD_CODING_MODEL` is unset, this value is used and also bypasses registry validation. Useful when the shell already exports `OLLAMA_MODEL=qwen3.5:397b-cloud`. |
| `OLLAMA_HOST` | `https://ollama.cloud` | Ollama endpoint. Set to `http://localhost:11434` to route requests through local Ollama (which proxies cloud models with its own credentials). |
| `LIGHTSQUAD_OLLAMA_TIMEOUT_S` | `120` | Per-task wall-clock cap in seconds. Increase for large models on slow hardware. |
| `LIGHTSQUAD_MAX_FIX_ATTEMPTS` | `3` | How many times the FixAgent loop retries a task after compile errors before escalating to HITL. |

### Contract Supervisor — Provider Selection

The lightsquad ContractSupervisor evaluates artifacts against a `TaskContract` and supports operator-selectable backends. Defaults inherit from the webshell's `LLM_BACKEND` so the supervisor matches the worker by construction. Override per-run:

| Variable | Values | Effect |
|----------|--------|--------|
| `LIGHTSQUAD_SUPERVISOR_PROVIDER` | `claude-code` (default) / `codex` / `ollama` / `openai` / `openrouter` / `litellm` / `vertex` / `openai-compatible` | Explicit supervisor backend. When unset, inherits `LLM_BACKEND`; ultimate fallback is `claude-code`. |
| `LIGHTSQUAD_SUPERVISOR_MODEL` | Model id in provider's namespace (e.g. `gpt-5`, `anthropic/claude-sonnet-4`, `gemini-2.5-flash`) | Required for OpenAI-compatible variants. For `claude-code` and `codex`, omit to use the CLI's authenticated default. |
| `LIGHTSQUAD_SUPERVISOR_BASE_URL` | URL | Overrides flavor default. Required for `openai-compatible` / `generic` / Vertex via LiteLLM. |
| `LIGHTSQUAD_SUPERVISOR_API_KEY` | Bearer token | Generic API key — checked before provider-specific env vars. |
| `LIGHTSQUAD_SUPERVISOR_TIMEOUT_S` | `300` | Per-evaluation wall-clock cap. |
| `OPENAI_API_KEY` / `OPENROUTER_API_KEY` / `LITELLM_API_KEY` | Bearer | Provider-specific fallback when `LIGHTSQUAD_SUPERVISOR_API_KEY` is unset. |
| `LIGHTARCHITECTS_CLAUDE_BIN` / `LIGHTARCHITECTS_CODEX_BIN` | Path | Override CLI binary location. |

**Provider coverage:** one `OpenAICompatible` variant handles native OpenAI, OpenRouter, LiteLLM proxy (which itself proxies Vertex AI / Bedrock / Anthropic / Gemini / Groq / etc.), Together, Fireworks, Azure OpenAI, Databricks. All speak the same OpenAI Chat Completions API with `response_format.json_schema.strict=true` for server-side structured-output enforcement.

**Northstar alignment:** Pillar 3 (MoE Platform — multi-model multi-agent routing with specialization and ensemble verification) is what this provider abstraction advances. Pillar 2 (Vibe Coding Orchestration — steer *other* coding agents) requires both supervisor and worker to be pluggable; supervisor is shipping; worker abstraction is a planned follow-on.

### Typical local dev invocation

```bash
LIGHTARCHITECTS_WEBSHELL_TOKEN=dev-token \
OLLAMA_HOST=http://localhost:11434 \
OLLAMA_MODEL=qwen3.5:397b-cloud \
./target/release/lightarchitects-webshell
```

Use `LIGHTSQUAD_MOCK_WORKERS=1` to test orchestration flow without burning LLM tokens.

### Ironclaw HITL Resolution — `POST /api/control`

Resolves an escalated ironclaw HITL decision (e.g., approve a dependency-add or reject a secret-write):

```
POST /api/control
Authorization: Bearer <token>
Content-Type: application/json

{ "command": "ironclaw_hitl_resolution", "escalation_nonce": "<nonce>", "approved": true, "operator_note": "approved via webshell" }
```

- **Auth**: same `AuthGuard` bearer token as all other API routes
- **Nonce**: UUID minted per escalation; carried in Telegram `callback_data` and POST body only — never logged, never displayed in UI (CWE-209)
- **Response codes**: 200 resolved · 400 invalid JSON · 401 bad token · 404 nonce not found · 409 already resolved
- **Handler**: `lightarchitects-webshell/src/events/control.rs → control_handler`

### Telegram HITL Relay

When an ironclaw build escalates a decision to HITL, the relay sends a Telegram message to the operator and waits for their response (approve/reject via callback button or webshell UI).

**Keychain configuration** (macOS; required for relay to activate):

| Keychain service | Account name | Value |
|---|---|---|
| `la-telegram-credential` | `bot_token` | Telegram bot token |
| `la-telegram-credential` | `chat_id` | Telegram chat ID (numeric string) |

If either keychain entry is absent, the relay silently deactivates — no relay is created and HITL resolution falls back to the webshell UI only.

- **Timeout**: 5 minutes per escalation; build continues if no response
- **Anti-replay**: SERAPH#3 pattern — `DashSet<Uuid>` of used nonces; duplicate callbacks rejected
- **Security**: `.without_url()` on all reqwest errors to prevent bot token leaking in logs
- **Source**: `lightarchitects-webshell/src/events/telegram_hitl_relay.rs`

## Build

```bash
make quality   # fmt --check + clippy + all tests (incl. bridge_mistral_injection.rs)
make fix       # auto-fix fmt + clippy
```

Frontend: `pnpm --dir ../lightarchitects-webshell-ui build` required before clippy (RustEmbed proc-macro needs `dist/`).

## AYIN Observability (copilot-ayin-instrumentation — shipped 2026-05-26)

Copilot turns now write AYIN spans via `ayin_traces_utils.ts` (frontend) and `copilot/mod.rs` (backend).

- `turn_span_id` is minted per turn and threaded through `CopilotTurnRequest` → backend → `write_span_to_disk`
- Tool-use spans reference `turn_span_id` as `parent_id` → enables Lineage Circuit visualization in AYIN
- `View in AYIN →` deeplink in `CopilotDrawer.svelte` navigates to AYIN dashboard at `:3742`
- Frontend tests: `src/__tests__/ayin-traces-utils.test.ts` (expanded to full span-diagram builder coverage)
