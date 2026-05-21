# Trust Model — webshell-mcp-host

Design memo for Phase 3–4 implementors. Documents the 5-layer security model and ScopeGovernor adaptation.

## 1. Threat Surface

The webshell MCP host spawns arbitrary stdio subprocesses (MCP servers) and forwards operator-crafted tool invocations to them. The trust surface has two attack vectors:

**Vector A — malicious server**: a community MCP server (e.g., compromised npm package `@drawio/mcp`) that:
- Reads files outside its declared scope via the spawned process
- Makes network calls to exfil data (CWE-918 SSRF from subprocess)
- Consumes unbounded resources (CWE-400)
- Exploits TOCTOU in path validation (CWE-367)

**Vector B — crafted invocation**: an operator (or a bug in the webshell UI) that sends a `tools/call` with args that:
- Inject OS commands via tool arguments (CWE-78)
- Traverse paths outside the server's allowed scope (CWE-22)
- Bypass per-tool authorization (CWE-285/CWE-863)

## 2. Five-Layer Defense

### Layer 1 — Spawn-time Scope Contract (ScopeGovernor)

Each server entry in `webshell-mcp.json` declares a `scope` block:

```json
{
  "name": "drawio",
  "command": "npx",
  "args": ["-y", "@drawio/mcp"],
  "scope": {
    "allowed_paths": ["/tmp/diagrams/"],
    "allowed_net_hosts": ["drawio.com"],
    "allowed_env_keys": ["HOME", "PATH"],
    "max_concurrent_calls": 3,
    "call_timeout_ms": 30000,
    "lifecycle_mode": "persistent"
  }
}
```

`ScopeGovernor::check_spawn(scope, env)` enforces:
- `allowed_env_keys` — env_clear() + whitelist on spawn (CWE-209 env exfil)
- `allowed_paths` — canonicalize-before-check (TOCTOU guard per Cookbook §63)
- `allowed_net_hosts` — registered pre-spawn (process-level firewall via scope)
- `max_concurrent_calls` — semaphore on tool invocation path

Adaptation from `seraph::scope_governor`: same 5-gate pattern (TTL + target + tool + concurrent + domain). The TTL gate is repurposed as `call_timeout_ms`. The domain gate maps to `allowed_net_hosts`.

Source: `seraph/src/scope_governor.rs` — primitives extracted directly. Phase 4 `trust.rs` adapts without forking.

### Layer 2 — OS-level Sandbox (macOS sandbox-exec)

Each spawned process runs under `sandbox-exec -f assets/sandbox-exec-default.sb` as the spawn wrapper command. The profile:
- Denies all file writes outside declared `allowed_paths`
- Denies all network calls except to `allowed_net_hosts` (DNS + TCP)
- Denies `exec` of child processes (CWE-78 subprocess injection)
- Denies `/dev/mem`, `/proc`, device access

Profile at `assets/sandbox-exec-default.sb`. Validated in Phase 2 against hostile fixture H1.

**macOS-specific**: `sandbox-exec` is a macOS-only API. Linux deployments use a seccomp-bpf profile (deferred to a follow-on build — see Tech Debt §7 in plan).

### Layer 3 — Process-group Containment

`spawner.rs` sets `setsid()` / `setpgid()` on the spawned process to isolate it in a new process group. On supervisor restart or timeout, `kill(-pgid, SIGKILL)` kills the entire process group — preventing orphan grandchildren from continuing after the supervised process exits.

This addresses the scenario where a hostile server forks a background process to outlive supervision.

### Layer 4 — Invocation-time Schema Validation + Per-tool Authorization

`schema_validate.rs` validates `tools/call` arguments against the pre-fetched `input_schema` before the call reaches the server. This prevents:
- Type-confusion attacks (sending a number where a path string is expected)
- Overlong inputs (array/string length bounds enforced by schema `maxItems`/`maxLength`)

Per-tool authorization: `scope.allowed_tools` (optional whitelist). If non-empty, only listed tool names may be invoked. Unknown tool names → `McpHostError::Scope` before any subprocess call.

### Layer 5 — Egress-time Atomic Access (check_and_open)

Any path used in a tool invocation (read file, write diagram) goes through `lightarchitects_arch::security::path::check_and_open(path, allowed_dir)`:
1. `canonicalize(path)` — resolves symlinks, eliminates `..`
2. `starts_with(allowed_dir)` — containment check
3. `open(path)` — atomic: no window between check and use (TOCTOU-free)

This is reused directly from `lightarchitects-arch`. Phase 3 adds it as a dep.

## 3. ScopeGovernor API Design (Phase 4)

```rust
/// Per-server scope enforcer. Immutable after construction.
pub struct ScopeGovernor {
    allowed_paths: Vec<PathBuf>,         // canonicalized at construction time
    allowed_net_hosts: Vec<String>,
    allowed_tools: Option<HashSet<String>>,
    max_concurrent: usize,
    call_timeout: Duration,
}

impl ScopeGovernor {
    /// Verify the server is allowed to spawn with the given env.
    /// Strips forbidden env keys (anything not in allowed_env_keys).
    pub fn check_spawn(&self, env: &HashMap<String, String>) -> Result<EnvMap, McpHostError>;

    /// Verify an invocation is authorized before forwarding to the server.
    pub fn check_invocation(&self, tool_name: &str, args: &Value) -> Result<(), McpHostError>;

    /// Verify a path arg is within declared scope (TOCTOU-safe).
    pub fn check_path(&self, path: &Path) -> Result<(), McpHostError>;

    /// Acquire a call slot (semaphore). Returns guard that releases on drop.
    pub async fn acquire_call_slot(&self) -> Result<CallSlot, McpHostError>;
}
```

## 4. CWE Zero-Exception Coverage

| CWE | Mitigation layer | Status |
|-----|-----------------|--------|
| CWE-78 (OS Command Injection) | Layer 2 sandbox denies exec; structural args (no shell) | Phase 2 design |
| CWE-22 (Path Traversal) | Layer 5 check_and_open with canonicalize-first | Phase 2 design |
| CWE-918 (SSRF) | Layer 1 allowed_net_hosts + Layer 2 sandbox | Phase 2 design |
| CWE-400 (Uncontrolled Resource Consumption) | Layer 4 semaphore + call_timeout | Phase 2 design |
| CWE-367 (TOCTOU) | Layer 5 atomic check_and_open | Phase 2 design |
| CWE-501 (Trust Boundary Violation) | Layer 1 scope contract enforced server-side | Phase 2 design |
| CWE-285 (Improper Authorization) | Layer 4 per-tool authz | Phase 2 design |
| CWE-306 (Missing Auth) | Existing AuthGuard on all /api/mcp/* routes | Phase 5 |
| CWE-209 (Error Info Leak) | Layer 4: McpHostError → structured response (no upstream bleed) | Phase 3 |

Reviewed by SERAPH (see gate-2 review record).
