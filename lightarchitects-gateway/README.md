# Light Architects Gateway

A single MCP binary that gives Claude Code access to a team of specialized AI teammates — security analysis, forensic investigation, knowledge management, observability, and more.

**4.0 MB binary. 211+ tests. 12 workflow presets. 60+ routable actions.**

## What It Does

The gateway is a single `tools` MCP tool that auto-routes requests to the right teammate:

```
tools {action: "guard", params: {code: "..."}}     → CORSO (AppSec)
tools {action: "helix", params: {sibling: "eva"}}   → SOUL (knowledge graph)
tools {action: "triage", params: {target: "..."}}    → QUANTUM (forensics)
tools {action: "preset", params: {name: "security"}} → switches routing priority
```

No manual routing needed — the gateway parses each action against typed SDK enums and finds the best teammate automatically.

## Teammates

| Name | Role | Actions |
|------|------|---------|
| **CORSO** | AppSec engineer — quality gates, code review, threat modeling | 19 |
| **EVA** | DevOps/DX engineer — CI/CD, deploy gates, memory enrichment | 9 |
| **SOUL** | Knowledge engineer — persistent knowledge graph, cross-session memory | 14 |
| **QUANTUM** | Forensic analyst — multi-source research, hypothesis testing | 9 |
| **SERAPH** | Red team operator — authorized pentest with scope governance | 6 |
| **AYIN** | Observability engineer — tracing, anomaly detection, decision audit | 3 |

## Presets (Workflow Archetypes)

Presets control **routing priority** — which teammate is checked first for ambiguous actions. SOUL is always enabled (knowledge graph is the foundation).

| Preset | Routing Priority | Use Case |
|--------|-----------------|----------|
| `software_engineering` | CORSO → EVA → SOUL → AYIN | Day-to-day coding (default) |
| `security` | SERAPH → CORSO → QUANTUM → SOUL → AYIN | Pentest + AppSec |
| `research` | QUANTUM → EVA → SOUL → AYIN | Deep investigation |
| `devops` | EVA → CORSO → SOUL → AYIN | CI/CD + deploy gates |
| `code_review` | CORSO → QUANTUM → SOUL | Focused PR review |
| `learning` | EVA → QUANTUM → SOUL | Codebase onboarding |
| `audit` | CORSO → SERAPH → SOUL | Compliance scanning |
| `forensics` | QUANTUM → SERAPH → SOUL | Incident response |
| `solo` | CORSO → SOUL | Minimal overhead |
| `observability` | AYIN → QUANTUM → SOUL | Runtime debugging |
| `full` | QUANTUM → CORSO → SERAPH → EVA → SOUL → AYIN | All teammates |
| `lean` | SOUL | Vault only |

Switch presets mid-session:

```
tools {action: "preset", params: {name: "forensics"}}
```

## Quick Start

### 1. Build

```bash
cd lightarchitects-sdk
cargo build --release -p lightarchitects-gateway
```

### 2. Install

```bash
cp target/release/lightarchitects ~/.lightarchitects/bin/lightarchitects
```

### 3. Configure Claude Code

Add to your Claude Code MCP config (`.mcp.json` or plugin):

```json
{
  "mcpServers": {
    "lightarchitects": {
      "command": "~/.lightarchitects/bin/lightarchitects",
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

### 4. First Run

On first startup, the gateway auto-generates `~/.lightarchitects/config.toml` with the `software_engineering` preset. Claude Code will see `first_run: true` in the discover output and can prompt you to choose a preset.

```bash
# Or configure manually:
lightarchitects initialize detect       # scan environment
lightarchitects initialize draft lean   # preview a config
lightarchitects initialize apply lean   # write it
```

## Core Actions

These are handled directly by the gateway (no teammate needed):

| Action | Description |
|--------|-------------|
| `read` | Read file contents |
| `write` | Create/overwrite file |
| `edit` | String replacement in file |
| `bash` | Execute shell command |
| `search` | Ripgrep file search |
| `glob` | Find files by pattern |
| `discover` | Gateway version, tools, teammate status |
| `ask_user` | Prompt user for input |
| `preset` | View or switch the active preset |
| `initialize` | Setup wizard |
| `import` | Import from external systems |
| `canon_check` | Validate decision against canon |
| `canon_evaluate` | Evaluate a canon candidate |

## Security

All 8 security controls from the SERAPH audit are enforced:

- **Path validation** — denied paths (.ssh, .gnupg, .aws), canonicalization, allowed_directories boundary
- **Write protection** — shell configs, system dirs, LaunchAgents blocked for writes
- **Bash blocklist** — rm -rf, pipe-to-shell, fork bombs, privilege escalation
- **Spar sandbox** — training exercises restricted to read/search/glob only
- **Binary integrity** — optional SHA-256 checksum verification before teammate spawn
- **File size limit** — 10 MiB cap on reads
- **Error sanitization** — home paths stripped from error messages
- **HTTP allowlist** — only localhost for Ollama/AYIN endpoints, anti-spoofing

### Preset Security Gates (SCRUM-ratified)

1. **Gate 1**: Security teammates (CORSO, SERAPH, AYIN) cannot be disabled by preset switch without HITL
2. **Gate 2**: Trust/scope levels live in config, not presets — preset switch cannot modify them
3. **Gate 3**: No `prompt_overlay` field in preset schema — prompts are identity-owned

## Configuration

`~/.lightarchitects/config.toml`:

```toml
[gateway]
version = "1.0.0"

active_preset = "software_engineering"

[routes.corso]
enabled = true
binary = "~/.corso/bin/corso"
tool_name = "corsoTools"
trust = "trusted"
scope = "own"

[routes.soul]
enabled = true
binary = "~/.soul/.config/bin/soul"
tool_name = "soulTools"
trust = "trusted"
scope = "all"

# ... more routes

[privacy]
tier = "local"    # local | hybrid | cloud
```

## CLI

```bash
lightarchitects routes                     # list enabled teammates
lightarchitects canon list                 # list ratified canons
lightarchitects canon check "use SQLite"   # check decision against canon
lightarchitects initialize detect          # scan environment
lightarchitects conductor start            # start autonomous task conductor
```

## Development

```bash
make quality    # fmt + clippy (pedantic) + tests
make fix        # auto-fix fmt + clippy
cargo test -p lightarchitects-gateway --all-features   # 211+ tests
```

## License

Apache-2.0
