//! Embedded Dockerfile and entrypoint script as Rust string constants.
//!
//! No standalone `Dockerfile` or `docker-compose.yml` in the project root —
//! the binary is self-contained. On first session spawn, the [`ImageManager`]
//! writes these strings to a temp directory and builds the agent image.

/// The agent container image — includes gateway binary, CLI, and CA certs.
pub const AGENT_DOCKERFILE: &str = r#"
FROM debian:12-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 ca-certificates git openssh-client nodejs npm \
    && rm -rf /var/lib/apt/lists/*
COPY lightarchitects /usr/local/bin/
COPY agent-entrypoint.sh /entrypoint.sh
RUN useradd -m -u 1000 agent
USER 1000:1000
WORKDIR /workspace
ENTRYPOINT ["/entrypoint.sh"]
"#;

/// Entrypoint script for the agent container.
///
/// Generates `.mcp.json` pointing at the webshell's WebSocket relay,
/// starts the gateway MCP in the background, then execs the requested agent CLI.
pub const AGENT_ENTRYPOINT: &str = r#"#!/bin/bash
set -euo pipefail
# Generate .mcp.json for the agent CLI to discover the gateway
cat > /workspace/.mcp.json <<'MCP_EOF'
{"mcpServers":{"lightarchitects-gui-bridge":{"command":"/usr/local/bin/lightarchitects","args":[],"env":{"LA_GUI_URL":"__LA_GUI_URL__","LA_BUILD_ID":"__LA_BUILD_ID__","LA_NOTIFY_TOKEN":"__LA_NOTIFY_TOKEN__"}}}}
MCP_EOF
# Replace placeholders from env vars
sed -i "s|__LA_GUI_URL__|$LA_GUI_URL|g" /workspace/.mcp.json
sed -i "s|__LA_BUILD_ID__|$LA_BUILD_ID|g" /workspace/.mcp.json
sed -i "s|__LA_NOTIFY_TOKEN__|$LA_NOTIFY_TOKEN|g" /workspace/.mcp.json
# Start gateway MCP in background, then exec the requested agent CLI
/usr/local/bin/lightarchitects &
exec "$@"
"#;
