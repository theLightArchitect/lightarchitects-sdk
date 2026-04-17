# lightarchitects-webshell

> **LOCAL DEVELOPMENT ONLY**
>
> This tool is built for personal, local use with **Claude Code as the host agent**.
> Claude Code is an Anthropic product — its CLI and API are subject to
> [Anthropic's usage policies](https://www.anthropic.com/legal/aup). Public
> distribution or use with a non-Claude-Code host requires a host swap and
> independent licensing review. See [Licensing](#licensing) below.

A local web GUI shell for the active coding agent. Embeds a live PTY-hosted
agent session (Claude Code by default) alongside a 3D session-helix panel
that grows in real time as the agent works.

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

### 1. Build the frontend bundle

The Rust binary embeds `web/dist/` at compile time. Build it first:

```bash
cd web
pnpm install
pnpm build
cd ..
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
  --port <PORT>         Port to listen on [default: 8733]
  --host-cmd <CMD>      Command to run in the PTY [default: claude]
  --cwd <PATH>          Working directory for the host command [default: $HOME]
  -h, --help            Print help
```

Environment variables:

| Variable | Purpose |
|---|---|
| `LIGHTARCHITECTS_WEBSHELL_TOKEN` | HMAC auth token (required for auth endpoints) |
| `RUST_LOG` | Log level: `error`, `warn`, `info`, `debug`, `trace` |
| `LIGHTARCHITECTS_HOME` | Override `~/lightarchitects/` root path |

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

### `GET /api/events`

Server-Sent Events stream. Requires auth. Streams `WebEvent` payloads as
`data: {json}\n\n`.

```bash
curl -N -H "Authorization: Bearer $LIGHTARCHITECTS_WEBSHELL_TOKEN" http://localhost:8733/api/events
```

Event types:

| `type` | Payload | Description |
|---|---|---|
| `ayin_span` | `{id, actor, action, timestamp, duration_ms, outcome}` | AYIN trace span |
| `ayin_status` | `{status: connected\|disconnected\|reconnecting, attempt?}` | AYIN connection lifecycle |
| `helix_entry` | `{path, event_kind: created\|modified}` | Vault file event (FS watcher) |
| `lag` | `{skipped: N}` | Broadcast channel lag (N events dropped) |

### `GET /api/terminal/ws`

PTY WebSocket bridge. Requires auth via sub-protocol `Bearer.<token>`. Raw
byte stream — send bytes to stdin, receive bytes from stdout/stderr.

```
ws://localhost:8733/api/terminal/ws
Sec-WebSocket-Protocol: Bearer.<token>
```

Concurrency cap: 4 simultaneous sessions. A 5th connection receives `503`.

### `GET /*`

Serves the embedded `web/dist/` bundle. Unknown paths fall back to
`index.html` to support React Router client-side routing.

---

## Development

```bash
# Run all quality gates (Rust + TypeScript)
make quality

# Run Rust tests only
cargo test --workspace

# Run TypeScript tests only
cd web && pnpm test

# Auto-fix fmt + clippy
make fix

# Build frontend in watch mode
cd web && pnpm dev
```

The backend recompiles with `cargo run -p lightarchitects-webshell`. Frontend
changes in `web/src/` are hot-reloaded by Vite's dev server at `:5173`. For
end-to-end dev, run both simultaneously and proxy the Vite dev server.

---

## Architecture

```
lightarchitects-webshell/
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── config.rs            # Config struct + Cli → Config
│   ├── auth.rs              # HMAC Bearer validation (constant-time)
│   ├── static_assets.rs     # rust-embed SPA handler
│   ├── server/mod.rs        # Axum router, AppState, run loop
│   ├── terminal/
│   │   ├── mod.rs           # PTY types re-export
│   │   ├── session.rs       # PtySession (portable-pty wrapper)
│   │   └── ws.rs            # WebSocket handler + concurrency cap
│   └── events/
│       ├── mod.rs           # WebEvent + broadcast channel
│       ├── types.rs         # WebEvent, TraceSpanSummary, AyinStatus
│       ├── ayin_client.rs   # AYIN SSE → broadcast (with reconnect)
│       ├── helix_watcher.rs # notify FS watcher → broadcast
│       └── sse_handler.rs   # GET /api/events SSE fan-out
└── web/
    ├── src/
    │   ├── App.tsx           # Split-pane layout
    │   ├── store/sceneState.ts  # Zustand store
    │   ├── hooks/useEventSource.ts  # SSE → store dispatch
    │   ├── three/            # 3D scene (R3F + Three.js)
    │   └── components/Terminal/  # xterm.js wrapper
    └── dist/                 # Embedded in binary at compile time
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
