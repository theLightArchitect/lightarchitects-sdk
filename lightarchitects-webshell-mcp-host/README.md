# lightarchitects-webshell-mcp-host

Generic MCP host library for the Light Architects webshell. Spawns declared stdio MCP servers, performs the `initialize` + `tools/list` handshake, caches the tool catalog, and exposes an async API consumed by the webshell HTTP surface.

## Architecture

```
webshell-mcp.json
      │
      ▼
 McpHostConfig::from_json()
      │
      ▼
 HostManager::from_config()
  ├── Supervisor (per server)
  │     ├── spawner.rs     — TokioChildProcess via rmcp
  │     ├── transport.rs   — stdio JSON-RPC transport
  │     └── supervisor.rs  — 7-state lifecycle FSM
  └── ToolCatalog          — cached tool inventory
        │
        ▼
   HostManager API
  ├── list_servers()
  ├── list_tools()
  ├── check_call_policy()  — Layer 4 pre-call gate
  └── invoke_tool()        — scope + schema check → MCP call
```

## Trust model (5 layers)

| Layer | Enforcement | CWE mitigated |
|-------|-------------|---------------|
| 1 | Env isolation (env_clear + whitelist) | CWE-209 |
| 2 | sandbox-exec (macOS, when `process_sandbox: "default"`) | CWE-78 |
| 3 | Process group isolation | CWE-400 |
| 4 | `ScopeGovernor` (tool allowlist, path prefix, net host) + `SchemaValidator` | CWE-22, CWE-918, CWE-285 |
| 5 | TOCTOU-safe binary path check | CWE-367 |

## Configuration

Place `~/.lightarchitects/webshell-mcp.json`. Schema mirrors `~/.claude/mcp.json` with an additive `scope` block:

```json
{
  "mcpServers": {
    "my-server": {
      "command": "/path/to/binary",
      "args": [],
      "env": {},
      "scope": {
        "lifecycle_mode": "persistent",
        "allowed_tools": null,
        "allowed_paths": ["/Users/me/projects"],
        "allowed_net_hosts": [],
        "allowed_env_keys": ["HOME", "PATH"],
        "max_concurrent_calls": 3,
        "call_timeout_ms": 30000
      }
    }
  }
}
```

`lifecycle_mode`:
- `"persistent"` (default) — server stays connected; calls are serialized via a mutex.
- `"one_shot"` — server is spawned per call (~30 ms latency penalty); used for servers that close stdio after `initialize`.

`allowed_tools: null` permits any tool; `[]` blocks all tools; `["tool_a"]` permits only `tool_a`.

`allowed_paths: []` (empty) blocks all path arguments. Always configure paths for servers that accept file arguments.

## HTTP surface (webshell)

Three routes are registered automatically when the config file is present:

```
GET  /api/mcp/servers  → list all managed servers + live state
GET  /api/mcp/tools    → list all cached tools across ready servers
POST /api/mcp/invoke   → { server, tool, input } → tool output
```

All routes require `AuthGuard`. Returns `503 {"error":"mcp_host not configured"}` when `webshell-mcp.json` is absent.

## Testing

```bash
cargo test -p lightarchitects-webshell-mcp-host
```

Hostile fixtures validate the trust boundary:
- `H1-b` — disallowed net host rejected (CWE-918 SSRF)
- `H1-c` — unlisted tool rejected (CWE-285)
- `H1-d` — path traversal rejected (CWE-22)

## Adding a new server

1. Add an entry to `~/.lightarchitects/webshell-mcp.json`.
2. Restart the webshell binary.
3. The new server appears in `GET /api/mcp/servers` as `Spawning` then `Ready`.
4. Its tools appear in `GET /api/mcp/tools` once the handshake completes.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| Server stuck in `Handshaking` | Binary doesn't speak MCP | Verify with `echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}' \| /path/to/binary` |
| Server in `CircuitOpen` | 5 consecutive restart failures | Check binary path, permissions, and `env` in config |
| `403 Scope` on invoke | Tool or path not in allowlist | Expand `scope.allowed_tools` or `scope.allowed_paths` |
| `502 BadGateway` on invoke | MCP call failed | Check server logs; `cargo test` with the server binary |
| 0 tools shown (auth-gated server) | Server requires `ANTHROPIC_API_KEY` at init | Add `"ANTHROPIC_API_KEY"` to `scope.allowed_env_keys` |
