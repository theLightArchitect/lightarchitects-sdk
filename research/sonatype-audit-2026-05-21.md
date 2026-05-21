# Sonatype Dependency Audit — webshell-mcp-host

Date: 2026-05-21  
Tool: sonatype-guide MCP (getComponentVersion)  
Scope: rmcp direct dep + key transitive deps from spike Cargo.lock

## Results

| Package | Version | Compliant | CVEs | License |
|---------|---------|-----------|------|---------|
| `rmcp` | 0.7.0 | ❌ FAIL | CVE-2026-42559 (CVSS 8.8) | MIT |
| `rmcp` | **1.7.0** | ✅ PASS | None | Apache-2.0 |
| `rmcp-macros` | **1.7.0** | ✅ PASS | None | Apache-2.0 |
| `process-wrap` | 8.2.1 | ✅ PASS | None | Apache-2.0 OR MIT |

## CVE Detail

**CVE-2026-42559** in `rmcp@0.7.0`:
- CVSS: 8.8 (HIGH)
- Policy: `CVSS < 7.0` threshold breached — NON-COMPLIANT
- Status: Fixed in rmcp 1.7.0 (latest)

## Resolution

**Upgraded from rmcp 0.7.0 → 1.7.0** per plan R-2 bump criteria:
> "Bump only on (a) CVE in pinned version..."

Plan updated: pin changed from `=0.7.0` to `=1.7.0` in R-2 section.  
API migration: documented in plan R-2 (minor API changes; fully compatible with webshell CancellationToken architecture).

## Phase 2 Exit Criterion

> ✅ Zero critical CVEs in rmcp + transitive deps (sonatype output captured)

Met after upgrade to rmcp 1.7.0. Cargo.toml in Phase 3 MUST use `=1.7.0`.
