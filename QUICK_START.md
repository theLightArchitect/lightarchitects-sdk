# Quick Start

Get a team of AI teammates in Claude Code in under 2 minutes.

## Prerequisites

- [Claude Code](https://claude.ai/code) installed
- Rust toolchain (`rustup`, `cargo`)
- At least one teammate binary built and deployed (see [Teammate Binaries](#teammate-binaries) below)

## Install

```bash
# Clone and build
git clone https://github.com/TheLightArchitects/lightarchitects-sdk.git
cd lightarchitects-sdk
cargo build --release -p lightarchitects-gateway

# Install the binary
mkdir -p ~/.lightarchitects/bin
cp target/release/lightarchitects ~/.lightarchitects/bin/
```

## Configure Claude Code

Add to your project's `.mcp.json` (or your Claude Code plugin config):

```json
{
  "mcpServers": {
    "lightarchitects": {
      "command": "/Users/YOU/.lightarchitects/bin/lightarchitects",
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

Replace `/Users/YOU` with your home directory path.

## First Run

Start Claude Code. The gateway auto-generates `~/.lightarchitects/config.toml` on first run with the `software_engineering` preset (CORSO + EVA + SOUL + AYIN).

Try it:

```
tools {action: "discover"}
```

You'll see your gateway version, active preset, enabled teammates, and available actions.

## Choose a Preset

Presets are workflow archetypes — they change which teammate handles ambiguous actions first.

```
tools {action: "preset"}                              # see all 12 presets
tools {action: "preset", params: {name: "security"}}  # switch to security mode
tools {action: "preset", params: {name: "solo"}}      # just CORSO + SOUL
```

| Preset | Best for |
|--------|----------|
| `software_engineering` | Daily coding (default) |
| `security` | Pentest + AppSec |
| `code_review` | PR review |
| `research` | Investigation |
| `forensics` | Incident response |
| `solo` | Minimal setup |
| `full` | Everything on |
| `lean` | SOUL vault only |

## Use It

Once configured, just talk to Claude Code naturally. The gateway routes automatically:

- "Review this code for security issues" → routes to CORSO
- "Research how this library works" → routes to QUANTUM
- "What did we decide about the auth module?" → routes to SOUL
- "Check if anything is slow" → routes to AYIN

Or use actions directly:

```
tools {action: "guard", params: {code: "fn main() { ... }"}}
tools {action: "helix", params: {significance_min: 8.0}}
tools {action: "search", params: {pattern: "TODO", glob: "*.rs"}}
```

## Teammate Binaries

Each teammate is a separate MCP binary. Build the ones you need:

| Teammate | Build | Deploy Path |
|----------|-------|-------------|
| CORSO | `cd CORSO/MCP/CORSO-DEV && make deploy` | `~/lightarchitects/corso/bin/corso` |
| EVA | `cd EVA/MCP/EVA-DEV/eva && make deploy` | `~/lightarchitects/eva/bin/eva` |
| SOUL | `cd SOUL/SOUL-DEV && make deploy` | `~/lightarchitects/soul/bin/soul` |
| QUANTUM | `cd QUANTUM/MCP/QUANTUM-DEV && cargo make deploy` | `~/lightarchitects/quantum/bin/quantum-q` |
| SERAPH | `cd SERAPH/MCP/SERAPH-DEV && make deploy-mac` | `~/lightarchitects/seraph/bin/seraph` |
| AYIN | `cd AYIN/AYIN-DEV && make deploy` | `~/lightarchitects/ayin/bin/ayin` (HTTP, not MCP) |

The gateway checks for binaries at startup. Missing binaries show as `"binary_missing"` in `discover` output — the gateway still works, those teammates just won't respond.

## What's Next

- Edit `~/.lightarchitects/config.toml` to enable/disable teammates
- Set `trust = "sandboxed"` for teammates that shouldn't run destructive commands
- Set `allowed_directories` to restrict file access to specific paths
- Use `tools {action: "initialize", params: {step: "detect"}}` for guided setup
