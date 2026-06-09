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
| `LIGHTARCHITECTS_WEBSHELL_TOKEN` | *(generated at startup)* | Auth token for the webshell HTTP API. Set to a known value to make `curl` tests predictable: `LIGHTARCHITECTS_WEBSHELL_TOKEN=test-token ./target/release/lightspace` |
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
./target/release/lightspace
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

## Runtime LiteLLM Provider Config (unified-litellm-router — shipped 2026-05-30)

`AppState.litellm_config: Arc<RwLock<LitellmConfig>>` — runtime-switchable LLM endpoint. No restart needed.

| Route | Auth | Purpose |
|-------|------|---------|
| `PUT /api/litellm/config` | Bearer | Store key in keychain + update AppState atomically |
| `GET /api/litellm/config` | Bearer | Return `{base_url, model, has_key, updated_at}` — never the raw key |
| `POST /api/litellm/chat` | Bearer | SSE chat stream via `LitellmConfig.build_provider()` |

**SSRF guard**: `base_url` must be `https://…` (any host) or `http://localhost`. Plain `http://` to remote IPs is rejected 400.

**Key pattern**: `security(1)` CLI subprocess writes to macOS keychain. In-memory: `secrecy::SecretString`. Subprocess workers receive `stub-la-{uuid}`, never the real key.

**Module**: `server/litellm_state.rs` (config struct + handlers) · `server/litellm_chat.rs` (SSE) · `auth/credential/litellm.rs` (keychain I/O)

## Autonomous Build Pipeline (ironclaw-autonomous-e2e — shipped 2026-05-30)

Wires `POST /api/builds { mode: "autonomous", cwd, waves: [[Task]] }` through the full IronClaw path:
`spawn_autonomous_build` → `OllamaCloudCodingProvider::execute_task` → `git commit` → `DecisionsWriter::append` → SSE broadcast.

### Key types

| Type | Module | Role |
|------|--------|------|
| `AppState.mock_workers` | `server/mod.rs` | `true` → no LLM calls; `false` → real `OllamaCloudCodingProvider`. Set by `AppState::for_test` (default `true`), override for E2E. |
| `OllamaCloudCodingProvider` | `events/coding_provider.rs` | Reads `OLLAMA_API_KEY` from env at worker-spawn time (NOT at server startup). |
| `DecisionsWriter` | `events/decisions_writer.rs` | HMAC-chained NDJSON ledger; path `$TMPDIR/la-decisions-{build_id}.ndjson`. |
| `WorkerSlotGauge` / `MergeAgentStatus` | `events/types.rs` | SSE events broadcast on slot changes and wave completions. |

### Test gate: `IRONCLAW_E2E=1` required opt-in

```bash
# Smoke tests (no LLM, no network) — always run with cargo test
cargo test --test smoke_autonomous_pipeline

# Real Ollama Cloud E2E — explicit opt-in required
IRONCLAW_E2E=1 OLLAMA_API_KEY=<key> cargo test --test autonomous_ollama_e2e -- --nocapture
```

`OLLAMA_API_KEY` alone does NOT activate E2E (prevents accidental 90s runs when key is in shell profile). Both `IRONCLAW_E2E=1` AND a non-empty `OLLAMA_API_KEY` are required.

### Smoke test pattern (`tower::ServiceExt::oneshot`)

```rust
let app = build_app(AppState::for_test(cfg, DockerCapability::Unavailable));
let resp = app.oneshot(Request::post("/api/builds").header(...).body(...).unwrap()).await.unwrap();
```

No TCP port required. `AppState::for_test` sets `mock_workers = true` — zero LLM calls.

**`doc_markdown` lint gotcha**: field names in `///` comments must be wrapped in backticks, e.g. `` (`mock_workers` = true) `` not `(mock_workers = true)`. The pre-commit hook enforces this via `clippy::doc_markdown`.

## Telegram HITL Relay (ironclaw-autonomous-e2e — shipped 2026-05-30)

`TelegramHitlRelay` (`events/telegram_hitl_relay.rs`) delivers escalation messages to a Telegram bot and processes callback answers from the operator.

### Keychain layout (service: `la-telegram-credential`)

| Account (`-a`) | Value |
|---|---|
| `bot-token` | Telegram Bot API token |
| `chat-id` | Operator Telegram chat ID |
| `webshell-auth-token` | Pre-shared token for `POST /api/control` |

All credentials read via `security find-generic-password -s la-telegram-credential -a <account> -w` (OA-3: no `keyring` crate, no env var fallback for production).

### Security invariants (NEVER violate)

- **CWE-316 fix**: `bot_token` and `webshell_auth_token` stored as `Arc<SecretString>` — zeroized on last-clone drop. Access only via `.expose_secret()`. Never stored as `String` or `Arc<String>`.
- **CWE-209 fix**: `reqwest` errors use `.without_url()` in `bot_api()` — bot token is never in an error string or log field.
- **escalation_nonce** (anti-replay): embedded in Telegram `callback_data` as URL-safe base64; `DashSet<String>` rejects duplicate callbacks (SERAPH#3). Nonce is NEVER logged, NEVER in UI, NEVER in error messages.
- **`POST /api/control` resolution path**: `{ command: "ironclaw_hitl_resolution", escalation_nonce, resolution, ... }` — NOT `/api/builds/:id/hitl/:call_id`. Nonce carried in request body only.

### `resolve_safe_path` — symlink-safe path canonicalization (CWE-22)

`events/control.rs::resolve_safe_path` uses ancestor-walk canonicalization: when the target path doesn't exist yet (new file), walk up to the nearest existing ancestor, canonicalize that, reattach the suffix. This catches symlinked-directory attacks (e.g., `cwd/link/ → /etc/`) even for non-existent target paths where `std::fs::canonicalize` would fail. Synthetic test paths like `/project/src/main.rs` fall through to lexical containment check since root `/` is always canonicalizable.

## Cockpit — Operator Domain Console (webshell-cockpit — shipped 2026-05-31)

The Cockpit is the primary operator surface at `/#/activity`. It replaces the generic Dashboard with a domain-preset × target-scope model: the operator picks a **preset** (their current role) and a **target** (what they're working on), and the Cockpit renders role-appropriate views.

### Preset × Target mental model

```
Preset   — WHAT ROLE AM I IN?     e.g. engineer / security / ops / research
Target   — WHAT AM I WORKING ON?  e.g. a PR, a build, a phase, a project
```

The combination gates which cards render. The **Engineer preset** renders the 4 domain zones:
- **NeedsAction** — builds/tasks requiring operator verb action (approve/review/unblock); 8-item cap
- **InFlight** — active builds and conductor tasks with progress bars and status dots
- **QuickActions** — one-click agent dispatch pre-filled with the active target context
- **Insights** — 4 non-obvious derived signals: confidence velocity, gate throughput, sibling failure rate, build age vs median

### Shortcuts

| Key | Action |
|-----|--------|
| `⌘T` | Open the Quick-Pick Palette — keyboard-first target selector; supports all 7 target types |
| `ESC` | Close Quick-Pick Palette or Copilot Drawer |
| Click HITL row | Sets `selectedTarget` to that PR/build |

### HITL Inbox

The `HITLInbox` card polls two endpoints every 60 seconds and merges results:
- `GET /api/gitforest/hitl-search` — GitHub PRs (⎌ icon) awaiting review
- `GET /api/conductor/hitl` — platform tasks (◈ icon) awaiting operator decision

Age colors: **green** = fresh (info severity), **amber** = warn, **red** = block. Clicking a row sets `selectedTarget` to that PR or build — downstream cards (PRMetadataBlock, PRVerbSurface) update accordingly.

### Copilot context injection

Every copilot message automatically carries `ui_context.cockpit = { preset, target }` via `snapshotContextForCopilot()` (`stores.ts:1090-1119`). The CopilotDrawer header button displays the active preset and, when a target is selected, appends `× <label>` (truncated at 22 chars). This means the AI assistant always knows what the operator is working on without requiring manual context-paste.

### Security invariants

- GitHub PAT never leaves the backend (`github_token_store`); never reaches the frontend; never logged
- All GitHub API calls funnel through `github_proxy.rs` — no direct browser-to-GitHub calls
- Fork PR confirmation modal (`ForkConfirmationModal.svelte`) cannot be bypassed via routing or programmatic invocation
- SSRF allowlist uses `(owner, repo)` tuple list — not repo-name-only; not env/config-driven

### Card roles (`data-card-role` taxonomy)

Every load-bearing Cockpit card declares `data-card-role` on its root element. Registry: `src/lib/cockpit/cardRoles.ts`. Exhaustiveness test: `src/__tests__/cockpit-card-roles.test.ts`. 15 roles total — adding a card requires updating both files and bumping the count assertion.

---

## HITL Question Bridge security constraints

`POST /api/question` + `POST /api/question/:id/answer` — Phase 6 hardened.

### F1 — Timing oracle (answer handler 404 path)
Both "ID never registered" and "ID registered but receiver dropped" return `404 Not Found`. Never return `410 Gone` on the answer path — `410` vs `404` leaks whether a UUID was ever valid (timing oracle). The `warn!` log on both 404 sub-paths is safe because `AuthGuard` is the first Axum extractor — unauthenticated callers are rejected before the registry is consulted.

### F4 — Answer allowlist (OWASP LLM01 prompt injection guard)
Answer labels are validated against `QuestionPending::questions[i].options` **before** the oneshot fires. The guard rejects: (a) labels not in the declared option set, (b) more answer vectors than declared questions, (c) absent metadata (`None` metadata + live registry entry = TTL race → 422, not bypass).

The validation runs server-side. The Svelte `QuestionCard` client-side guard is UX-only — never trust it as a security control.

### F5 — Body size limit
Both routes carry `DefaultBodyLimit::max(32 * 1024)` (32 KB). Do not remove this limit; large payloads from a compromised gateway could exhaust memory before deserialization.

### SseGuard: Send assertion
`SseGuard` crosses an `.await` boundary inside `drive_agent_stream`. A compile-time assertion in `src/agent/sse.rs` (`fn assert_send::<SseGuard>()`) enforces that `SseGuard` remains `Send`. If you ever add a `!Send` field (e.g., `Rc<T>`, `MutexGuard`, `tracing::span::EnteredSpan`) the test crate will fail to compile at the call site — the error manifests at `assert_send::<SseGuard>()`, not at the field definition.

### Single-operator contract (`QuestionPending`)
`QuestionPending` has no `session_id` or `build_id`. `SseGuard::drop()` therefore drains **all** pending questions globally on any SSE disconnect. This is correct for the single-operator model (one human, one browser tab). If the webshell is ever extended to multi-operator sessions, `QuestionPending` MUST gain a `session_id` field and `SseGuard` must scope its drain to the disconnecting session — otherwise tab-A's disconnect cancels tab-B's pending questions.

### headless_policy caution
Skills that call `question` with `headless_policy: AutoFirst` will auto-approve the first option if the webshell is unreachable. Never use `AutoFirst` on questions where the first option is a destructive or security-gated action. Before promoting a skill to production, audit: `grep -r "AutoFirst" ~/lightarchitects/skills/` should return zero matches on gated paths.

### Answer re-entry trust boundary (LÆX canon candidate)
`POST /api/question/:id/answer` is the first webshell surface where operator-supplied data flows
directly into an LLM `tool_result` payload. The F4 allowlist (label membership + count + single-select
cardinality) is the primary guard. This pattern is a **LÆX Platform Canon XIV candidate** — trust
boundary between the operator browser and the LLM context. Before any future answer-path refactor,
verify: (a) the F4 guard still runs before the oneshot fires, (b) no raw user string reaches the
LLM context without going through the allowlist, (c) the single-operator contract holds.

### Registry and submission limits (Phase 7 hardening)
Two caps guard against resource exhaustion from authenticated-but-misbehaving callers:
- `MAX_QUESTIONS_PER_SUBMIT = 20` — questions per `POST /api/question` body.
- `MAX_CONCURRENT_QUESTIONS = 32` — simultaneous pending questions across all gateway calls.
Both are `pub(crate)` constants in `src/server/question_routes.rs`. Raise them only after measuring
memory impact at the new ceiling under sustained 300 s long-polls.
