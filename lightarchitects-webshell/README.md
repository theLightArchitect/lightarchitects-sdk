# lightarchitects-webshell

> **LOCAL DEVELOPMENT ONLY**
>
> This tool is built for personal, local use with **Claude Code as the host agent**.
> Claude Code is an Anthropic product — its CLI and API are subject to
> [Anthropic's usage policies](https://www.anthropic.com/legal/aup). Public
> distribution or use with a non-Claude-Code host requires a host swap and
> independent licensing review. See [Licensing](#licensing) below.

A local web GUI shell for the active coding agent. Embeds a live PTY-hosted
Claude Code session (or Ollama Cloud alternative) alongside the Svelte Mockcli
frontend. The coding agent can manipulate the UI directly via `ui_*` MCP tools
published by `lightarchitects-gateway`.

```
┌─────────────────────────┬──────────────────────────┐
│                         │                          │
│    claude terminal      │    3D session helix      │
│   (xterm.js + PTY)      │  (steps + retrieval orb) │
│                         │                          │
└─────────────────────────┴──────────────────────────┘
               localhost:8733
```

**Build**: luminous-weaving-nautilus | **Status**: local-dev-only

---

## Prerequisites

| Dependency | Required for | Install |
|---|---|---|
| [Rust 1.87+](https://rustup.rs) | Backend binary | `rustup update` |
| [Node.js 20+](https://nodejs.org) + [pnpm](https://pnpm.io) | Frontend build | `npm i -g pnpm` |
| [Claude Code](https://claude.ai/code) | Default host agent | `npm i -g @anthropic-ai/claude-code` |
| [AYIN](../AYIN/AYIN-DEV/) | Live helix events (optional) | see AYIN README |

AYIN is optional. Without it, the helix panel falls back to filesystem watch
on `~/lightarchitects/soul/helix/` for new vault entries.

---

## Quick Start

### 1. Build the Mockcli frontend

The Rust binary embeds `~/Projects/Lightarchitectmockcli/dist/` at compile time via `rust-embed`. Build it first:

```bash
cd ~/Projects/Lightarchitectmockcli
pnpm install --frozen-lockfile
pnpm build
```

Or use the Makefile shortcut from the webshell crate root:
```bash
make mockcli
```

### 2. Build and deploy the binary

```bash
make deploy
# → binary at ~/lightarchitects/webshell/bin/lightarchitects-webshell
```

Or, for a one-shot release build without deploying:

```bash
cargo build --release -p lightarchitects-webshell
```

### 3. Set the auth token

The webshell requires an HMAC token to authenticate both the SSE stream and
the PTY WebSocket. Generate one and export it:

```bash
export LIGHTARCHITECTS_WEBSHELL_TOKEN=$(openssl rand -hex 32)
echo "Your token: $LIGHTARCHITECTS_WEBSHELL_TOKEN"
```

Add it to your shell profile to persist between sessions:

```bash
echo "export LIGHTARCHITECTS_WEBSHELL_TOKEN=$(openssl rand -hex 32)" >> ~/.zshrc
```

### 4. Run

```bash
lightarchitects-webshell --port 8733
# or with a custom host command:
lightarchitects-webshell --port 8733 --host-cmd /path/to/your-agent
```

Open [http://localhost:8733](http://localhost:8733) in your browser.

---

## Auth Token Setup

All authenticated endpoints require an `Authorization: Bearer <token>` header.
The PTY WebSocket uses the token as the WebSocket sub-protocol for browsers
(which cannot set arbitrary headers on WebSocket upgrades).

| Endpoint | Auth method |
|---|---|
| `GET /api/auth-check` | `Authorization: Bearer <token>` |
| `GET /api/events` | `Authorization: Bearer <token>` |
| `GET /api/terminal/ws` | WebSocket sub-protocol: `Bearer.<token>` |
| `GET /api/health` | Unauthenticated |
| `GET /*` | Unauthenticated (static assets) |

The token is read from the `LIGHTARCHITECTS_WEBSHELL_TOKEN` environment variable at startup.
If unset, the server starts but all auth-gated endpoints return `401`.

**Security note**: The token is never logged, never echoed in error messages,
and is compared using constant-time comparison to prevent timing attacks. Keep
it out of shell history (`export LIGHTARCHITECTS_WEBSHELL_TOKEN=...`, not `LIGHTARCHITECTS_WEBSHELL_TOKEN=...
lightarchitects-webshell`).

---

## CLI Reference

```
lightarchitects-webshell [OPTIONS]

Options:
  --port <PORT>              Port to listen on [default: 8733]
  --host-cmd <CMD>           Command to run in the PTY [default: claude]
  --cwd <PATH>               Working directory for the host command [default: $HOME]
  --agent <TEMPLATE>         Default claude --agent template (e.g. corso, eva)
  --backend <anthropic|ollama>  Claude backend [default: anthropic]
  --ollama-base-url <URL>    Ollama Anthropic-compat base URL
  --ollama-model <MODEL>     Ollama model name (e.g. qwen3-coder:480b-cloud)
  --ollama-key <KEY>         Ollama auth token (stored in platform data dir)
  -h, --help                 Print help
```

Environment variables:

| Variable | Purpose |
|---|---|
| `LIGHTARCHITECTS_WEBSHELL_TOKEN` | HMAC auth token (required for auth endpoints) |
| `RUST_LOG` | Log level: `error`, `warn`, `info`, `debug`, `trace` |
| `LIGHTARCHITECTS_HOME` | Override `~/lightarchitects/` root path |

Per-PTY env vars injected into each spawned Claude Code process:

| Variable | Purpose |
|---|---|
| `LA_GUI_URL` | Webshell base URL — gateway reads this to POST notify events |
| `LA_BUILD_ID` | UUID of this build session |
| `LA_NOTIFY_TOKEN` | Per-build HMAC token for `POST /api/builds/:id/notify` |
| `ANTHROPIC_BASE_URL` | Set only for Ollama backend — Anthropic-compat endpoint |
| `ANTHROPIC_MODEL` | Set only for Ollama backend |
| `ANTHROPIC_AUTH_TOKEN` | Set only for Ollama backend — never logged |

---

## API Surface

### `GET /api/health`

Unauthenticated liveness probe. Returns `200 OK` with body `ok`.

```bash
curl http://localhost:8733/api/health
# ok
```

### `GET /api/auth-check`

Validates the Bearer token. Returns `200` on match, `401` on mismatch or
missing header.

```bash
curl -H "Authorization: Bearer $LIGHTARCHITECTS_WEBSHELL_TOKEN" http://localhost:8733/api/auth-check
# 200 OK
```

### Multi-build routes (new in v0.2.0)

```bash
# Create a build session (returns build_id)
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"cwd":"/tmp/myproject"}' \
  http://localhost:8733/api/builds

# Get build details (no notify_token — delivered only via LA_NOTIFY_TOKEN env)
curl -H "Authorization: Bearer $TOKEN" http://localhost:8733/api/builds/<id>

# Per-build SSE — streams WebEvent payloads for one build
curl -N -H "Authorization: Bearer $TOKEN" http://localhost:8733/api/builds/<id>/events

# Per-build PTY WebSocket (binary frames = PTY I/O; text = resize JSON)
ws://localhost:8733/api/builds/<id>/terminal/ws
Sec-WebSocket-Protocol: bearer.<token>

# Gateway notify endpoint — used by lightarchitects-gateway ui_* tools
curl -X POST -H "x-la-notify-token: <per-build-token>" \
  -H "Content-Type: application/json" \
  -d '{"type":"focus_pillar","pillar":"ARCH"}' \
  http://localhost:8733/api/builds/<id>/notify
```

### `GET /api/events`

Global SSE stream (legacy, pre-multi-build). Requires auth.

### `GET /*`

Serves the embedded Mockcli `dist/` bundle. Unknown paths fall back to
`index.html` for Svelte client-side routing.

**Security**: the `x-la-notify-token` is per-build and distinct from the global
Bearer token. Using the global Bearer on a `/notify` endpoint returns `401` —
this prevents browser-held credentials from forging gateway events.

---

## Development

```bash
# Run all quality gates (Rust + Mockcli TypeScript)
make quality

# Rust tests only
cargo test -p lightarchitects-webshell

# Mockcli TypeScript tests only
cd ~/Projects/Lightarchitectmockcli && pnpm test:run

# Auto-fix fmt + clippy
make fix

# Mockcli dev server (proxies /api/* to :8733)
cd ~/Projects/Lightarchitectmockcli && LA_BACKEND_URL=http://localhost:8733 pnpm dev
```

For end-to-end dev: run the webshell binary (`make run`) in one terminal, and the
Mockcli dev server (above) in another — the Vite proxy forwards all `/api/*` calls to
the live Axum server while HMR keeps the frontend hot-reloaded.

---

## Architecture

```
lightarchitects-webshell/
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── config.rs            # Config struct + AgentSession enum (ClaudeCode × {Anthropic, Ollama})
│   ├── auth.rs              # Bearer + constant-time notify token validation
│   ├── session.rs           # BuildSession + BuildRegistry (DashMap<Uuid, Arc<BuildSession>>)
│   ├── mcp_config.rs        # Atomic .mcp.json writer (lightarchitects-gui-bridge)
│   ├── mock_data.rs         # Phase D stub routes (reads + 501 writes)
│   ├── static_assets.rs     # rust-embed SPA handler (embeds Mockcli dist/)
│   ├── server/mod.rs        # Axum router, AppState, CORS, run loop
│   ├── terminal/
│   │   ├── session.rs       # run_session — PTY spawn + env/argv injection
│   │   └── ws.rs            # Global + per-build WS handlers
│   └── events/
│       ├── types.rs         # WebEvent (GatewayNotify, AyinSpan, HelixEntry, …)
│       ├── notify.rs        # POST /api/builds/:id/notify handler
│       ├── builds_handler.rs# POST /api/builds + GET /api/builds/:id
│       ├── sse_handler.rs   # Global + per-build SSE fan-out
│       ├── ayin_client.rs   # AYIN SSE → broadcast
│       └── helix_watcher.rs # FS watcher → broadcast
└── ~/Projects/Lightarchitectmockcli/   ← embedded at compile time via rust-embed
    ├── src/                 # Svelte 5 + Tailwind frontend
    └── dist/                # Built bundle (pnpm build → make mockcli)
```

---

## lÆx0 Integration (Aspirational)

The `--host-cmd` flag is designed for a future build where lÆx0 replaces
Claude Code as the host agent. Once lÆx0 reaches PTY parity, the webshell
becomes a universal frontend for any agent that speaks a PTY interface.

```bash
# Future — not yet available
lightarchitects-webshell --host-cmd laex0 --cwd ~/Projects/myproject
```

This path is explicitly deferred to a future build (`lÆx0-as-host`) to keep
nautilus scoped.

---

## Licensing

The webshell crate itself is released under the same license as the
`lightarchitects-sdk` workspace.

**The default host command** (`claude` CLI) is an Anthropic product governed
by [Anthropic's Terms of Service](https://www.anthropic.com/legal/consumer-tos)
and [Acceptable Use Policy](https://www.anthropic.com/legal/aup). Bundling,
distributing, or commercially deploying this tool with `claude` as the
default host requires independent review of those terms.

For public distribution: swap `--host-cmd` to a non-Claude agent and review
the applicable license for that agent.
