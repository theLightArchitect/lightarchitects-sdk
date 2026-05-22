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

## Build

```bash
make quality   # fmt --check + clippy + all tests (incl. bridge_mistral_injection.rs)
make fix       # auto-fix fmt + clippy
```

Frontend: `pnpm --dir ../lightarchitects-webshell-ui build` required before clippy (RustEmbed proc-macro needs `dist/`).
