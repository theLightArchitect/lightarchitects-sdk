# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report security issues privately to **kf.tan@lightarchitects.io**.

Include:
- A description of the vulnerability and its potential impact
- Steps to reproduce or a minimal proof of concept
- The crate and version affected

### Response SLA

| Milestone | Target |
|-----------|--------|
| Acknowledgement | 48 hours |
| Initial assessment | 5 business days |
| Fix or mitigation | 30 days (critical: 7 days) |
| Public disclosure | Coordinated with reporter |

### Scope

In scope: `lightarchitects` (the unified SDK library) and `lightarchitects-webshell` (the local
web GUI). The `lightarchitects-gateway` binary is workspace-excluded from this repository and
available on request — security reports for the gateway are also accepted via the contact above.

Out of scope: vulnerabilities in upstream dependencies (report those to the upstream
project directly); issues that require physical access to the machine running the binary.

### CVE Policy

We request CVEs for vulnerabilities that affect published crates and have a CVSS score ≥ 4.0.
Internal-only issues (those that cannot be triggered without already having local code execution)
are documented in the changelog but do not receive CVEs.

### Cryptographic Standards

This crate uses `RustCrypto` primitives (AES-256-GCM, Ed25519, HKDF-SHA256, HMAC-SHA256).
Key material is zeroed on drop via `zeroize`. If you discover a cryptographic weakness in
our usage of these primitives, please report it — even if the underlying algorithm is sound.
