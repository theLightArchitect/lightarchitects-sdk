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

## Build

```bash
make quality   # fmt --check + clippy + all tests (incl. bridge_mistral_injection.rs)
make fix       # auto-fix fmt + clippy
```

Frontend: `pnpm --dir ../lightarchitects-webshell-ui build` required before clippy (RustEmbed proc-macro needs `dist/`).
