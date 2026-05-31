# Dep Audit — webshell-hitl-bridge Phase 0
Date: 2026-05-31
Tool: sonatype-guide getRecommendedComponentVersions

## rmcp@1.7.0
- DeveloperTrustScore: 85
- Direct vulnerabilities: 0
- Transitive vulnerabilities: 0
- License: MIT
- Outcome: NO_DATA_FOR_VERSION (already at latest)
- Decision: SAFE to add server features to lightarchitects-webshell

## dialoguer
- Recommended version: 0.11.0 (DeveloperTrustScore: 96)
- License: MIT (threat level 0)
- Vulnerabilities: 0
- Decision: USE 0.11.0 in lightarchitects-gateway/Cargo.toml

## Features to add to webshell rmcp dep
rmcp = { version = "=1.7.0", features = ["server", "macros", "transport-streamable-http-server", "transport-streamable-http-server-session", "schemars"] }
