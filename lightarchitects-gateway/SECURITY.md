# SECURITY.md — platform-api-v1 SERAPH Findings Report

**Engagement**: platform-api-v1 Phase 4 Hardening  
**Date**: 2026-05-04  
**Scope**: `lightarchitects-gateway` HTTP mode (`localhost:8080`)  
**Auditor**: SERAPH (red-team review) + CORSO GUARD (AppSec)  
**Classification**: Internal — Light Architects Engineering

---

## Scope Boundary

- **In scope**: HTTP surface on `localhost:8080` — all routes under `/v1/platform/*`, `/v1/admin/*`, `/v1/vault/*`
- **Out of scope**: MCP stdio protocol, Arena scheduler, Conductor queue, Neo4j instance itself
- **Binding constraint**: localhost-only; no public network exposure. CORS allowlist enforces `127.0.0.1` and `localhost` origins only.

---

## Executive Summary

Six findings were identified during Phase 4 review. All six have been remediated. One informational finding (F-INFO-1) has no remediation path and is accepted by design. No open findings remain.

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| F-CRIT-1 | **CRITICAL** | Admin endpoint had no authentication | ✅ Fixed (H4) |
| F-HIGH-1 | **HIGH** | Path params not validated — traversal characters accepted | ✅ Fixed (H2) |
| F-MED-1 | **MEDIUM** | Admin `kind` field accepted arbitrary strings | ✅ Fixed (H3) |
| F-MED-2 | **MEDIUM** | Admin `version` field accepted arbitrary strings | ✅ Fixed (H3) |
| F-MED-3 | **MEDIUM** | Payload size uncapped before Neo4j write | ✅ Fixed (Polish) |
| F-LOW-1 | **LOW** | Single rate limiter for all endpoint tiers | ✅ Fixed (H5) |
| F-INFO-1 | **INFO** | Migration DDL loaded from filesystem at startup | ℹ️ Accepted |

---

## Findings

### F-CRIT-1 — Admin endpoint had no authentication

**Severity**: CRITICAL  
**Location**: `src/http/routes/admin.rs` — `POST /v1/admin/canon/upload`  
**Phase found**: Phase 3 (pre-Phase 4)

**Description**: The `upload_canon` endpoint that writes `PlatformEntry` nodes to Neo4j required no authentication. Any process able to reach `localhost:8080` could upsert arbitrary canon content, overwrite existing entries, and poison cached responses served to sibling clients.

**Attack path**:
1. Local process connects to `localhost:8080`
2. `POST /v1/admin/canon/upload` with `{"path":"canon/builders-cookbook","content_text":"<attacker content>"}`
3. Neo4j `MERGE` overwrites the canonical entry
4. All subsequent `GET /v1/platform/canon/canon/builders-cookbook` responses serve attacker content until TTL expires or binary restarts

**Remediation** (H4):
- `admin_token: Option<SecretBox<String>>` stored in `PlatformState`; loaded from macOS keychain (`soul-neo4j-local/admin-token`) with env var fallback (`LIGHTARCHITECTS_ADMIN_TOKEN`)
- `upload_canon` handler checks `x-admin-token` header before reading any body fields
- Comparison uses `subtle::ConstantTimeEq` — timing-safe; prevents oracle attacks on token length/prefix
- When `admin_token` is `None` (token not configured), endpoint returns 503 instead of proceeding unauthenticated
- `x-admin-token` added to CORS `allow_headers`

**Residual risk**: Token stored in plaintext in macOS keychain (Keychain Services). Acceptable for localhost-only deployment; Keychain access requires local user authentication.

---

### F-HIGH-1 — Path params not validated — traversal characters accepted

**Severity**: HIGH  
**Location**: `src/http/routes/platform.rs` — handlers `canon_get`, `agents_get`, `agents_strands_get`, `skills_get`, `standards_get`  
**Phase found**: Phase 4 audit

**Description**: Path parameters (`:name`, `:sibling`) were passed directly to neo4rs without character-level validation. While neo4rs parameterization prevents Cypher injection at the protocol level, path params containing `..`, `/`, `\`, or control bytes appeared verbatim in log fields and audit entries, could leak internal path structure through error messages, and violated the principle of fail-fast input validation at the API boundary.

**Remediation** (H2):
- `validate_path_param(val: &str) -> Option<Response>` added — rejects on: empty string, contains `..`, contains `/`, contains `\`, any byte outside printable ASCII range `0x20–0x7E`
- Called at the top of all 5 affected handlers before any cache lookup or Neo4j query
- Returns HTTP 400 with `{"error":{"code":"invalid_path","status":400}}`

**Residual risk**: None. Parameterization provides the structural guarantee; validation provides defense-in-depth and fail-fast boundary enforcement.

---

### F-MED-1 — Admin `kind` field accepted arbitrary strings

**Severity**: MEDIUM  
**Location**: `src/http/routes/admin.rs` — `upload_canon`  
**Phase found**: Phase 4 audit

**Description**: The `kind` field on `PlatformEntry` nodes had no server-side validation. An authenticated caller could write arbitrary `kind` values (e.g. `"__proto__"`, `"../etc/passwd"`) which would then be returned verbatim to all clients reading those entries. Downstream consumers relying on a closed `kind` enum for routing decisions could be confused or exploited.

**Remediation** (H3):
- `ALLOWED_KINDS: &[&str] = &["canon", "standard", "template", "skill"]` compile-time constant
- Validated before Neo4j write; returns HTTP 422 with `{"error":{"code":"invalid_kind","allowed":[...],"status":422}}`

**Residual risk**: None. Allowlist is exhaustive; new kinds require a code change.

---

### F-MED-2 — Admin `version` field accepted arbitrary strings

**Severity**: MEDIUM  
**Location**: `src/http/routes/admin.rs` — `upload_canon`  
**Phase found**: Phase 4 audit

**Description**: The `version` field accepted any string, including empty strings, excessively long values, and strings containing characters meaningful to downstream tooling (semver parsers, dependency resolvers). An authenticated attacker could store malformed versions that break version-comparison logic in clients.

**Remediation** (H3):
- `is_valid_semver(v: &str) -> bool` — requires exactly 3 dot-separated non-negative integers (`MAJOR.MINOR.PATCH`)
- Validated before Neo4j write; returns HTTP 422 with `{"error":{"code":"invalid_version","detail":"...","status":422}}`

**Residual risk**: Low. Validation is intentionally strict (no pre-release suffixes, no build metadata). If semver extensions are needed later, `is_valid_semver` is the single change point.

---

### F-MED-3 — Payload size uncapped before Neo4j write

**Severity**: MEDIUM  
**Location**: `src/http/routes/admin.rs` — `upload_canon`  
**Phase found**: Polish phase (pre-H3)

**Description**: `content_text` and `content_json` were written to Neo4j without a size check. A large payload (100 MB+) would be accepted, buffered by axum, serialized to JSON, and sent over the Bolt connection, potentially exhausting Neo4j heap or triggering OOM on the gateway process.

**Remediation** (Polish):
- `MAX_CONTENT_BYTES: usize = 512 * 1024` (512 KB) compile-time constant
- `content_bytes = content_text.len().max(content_json_str.len())` — checked before any Neo4j call
- Returns HTTP 413 with `{"error":{"code":"content_too_large","max_bytes":524288,"status":413}}`

**Note**: axum's default request body limit (~2 MB) provides an outer guard; the 512 KB check is a stricter, application-level limit that fires before Neo4j is touched.

**Residual risk**: None for current threat model. If large content is a legitimate future requirement, `MAX_CONTENT_BYTES` is a single constant.

---

### F-LOW-1 — Single rate limiter for all endpoint tiers

**Severity**: LOW  
**Location**: `src/http/middleware/rate_limit.rs`, `src/http/state.rs`  
**Phase found**: Phase 4 audit

**Description**: A single `PlatformRateLimiter` at 100 req/min applied to all endpoints identically. This allowed a client to exhaust the helix query budget (which is computationally expensive against Neo4j) at the same rate as a cheap health check, and to make 10 write attempts per second without additional friction.

**Remediation** (H5):
- Replaced single limiter with 3 tiered `governor::RateLimiter` instances:
  - `read_limiter` — 100/min — `/v1/platform/canon/*`, agents, skills, standards
  - `helix_limiter` — 20/min — `/v1/platform/helix/query`, `/v1/vault/info`
  - `write_limiter` — 10/min — `/v1/admin/*`
- Path-aware middleware (`rate_limit_middleware`) dispatches to the correct limiter based on URI prefix
- `/v1/platform/health` is exempt — liveness probes must not consume quota
- All limiters keyed per `IpAddr` (extracted from `ConnectInfo<SocketAddr>`)

**Residual risk**: Rate limits are conservative for localhost traffic. If the gateway is ever proxied or exposed externally, consider adding `X-Forwarded-For` awareness and per-org-id limits.

---

### F-INFO-1 — Migration DDL loaded from filesystem at startup

**Severity**: INFO (accepted)  
**Location**: `src/http/neo4j.rs` — `apply_migrations`  
**Phase found**: Phase 4 audit

**Description**: Platform schema migrations are loaded from `.cypher` files on the filesystem at startup (`migrations/platform/*.cypher`). If an attacker can write to the migrations directory before the binary starts, they could inject arbitrary Cypher DDL. The migration files contain no user input and are executed as DDL (not data queries).

**Accepted — no remediation**:
- The threat requires local filesystem write access, which implies full system compromise
- Migration files are controlled by the binary's working directory; in deployment this is `~/.lightarchitects/bin/` — same privilege level as the binary itself
- Migrations are idempotent and tracked by `Migration` nodes, so re-injection of existing files is a no-op
- Embedding migrations as `include_str!` compile-time constants would eliminate this vector entirely and is the recommended future improvement if the threat model expands to shared-host deployments

---

## Attack Surface Summary

| Surface | Exposure | Guard |
|---------|----------|-------|
| `GET /v1/platform/*` | localhost only | Path validation + tiered rate limit (100/min) |
| `GET /v1/platform/helix/query` | localhost only | Path validation + helix rate limit (20/min) |
| `GET /v1/vault/info` | localhost only | Helix rate limit (20/min) |
| `POST /v1/admin/canon/upload` | localhost + CORS | Token auth (constant-time) + kind/version/size validation + write rate limit (10/min) |
| `GET /v1/platform/health` | localhost only | No auth, no rate limit (by design — liveness probe) |
| Neo4j connection | localhost bolt:// | Credentials in keychain; not user-configurable at runtime |

---

## Security Controls Inventory

| Control | Implementation | Location |
|---------|---------------|----------|
| Admin authentication | `SecretBox<String>` + `subtle::ConstantTimeEq` | `admin.rs`, `state.rs` |
| Token source hierarchy | Keychain → env var → None (503) | `main.rs:load_admin_token` |
| Path traversal guard | Reject `..` `/` `\` non-printable | `platform.rs:validate_path_param` |
| Kind allowlist | Static `&[&str]` — 4 values | `admin.rs:ALLOWED_KINDS` |
| Version format | 3-part integer check | `admin.rs:is_valid_semver` |
| Payload size cap | 512 KB hard limit | `admin.rs:MAX_CONTENT_BYTES` |
| Cypher parameterization | neo4rs `.param()` — Bolt protocol | All 16 query call sites |
| Tiered rate limiting | 3 governor limiters — 100/20/10 req/min | `rate_limit.rs` |
| CORS origin allowlist | `127.0.0.1:{5173,8080}`, `localhost:5173` | `http/mod.rs` |
| Admin audit log | JSONL append-only outside Neo4j | `admin.rs:write_audit_log` |

---

## Recommendations for Phase 5+

1. **Embed migrations as `include_str!`** — eliminates F-INFO-1's filesystem dependency at zero runtime cost.
2. **Add `X-Org-Id` validation** — currently accepted as-is from headers. A character and length check (printable ASCII, ≤128 bytes) would close the last unvalidated string input path.
3. **Log 4xx responses at `warn!` level** — rejected path params and rate-limited IPs currently surface only in AYIN traces. A single `warn!` at the middleware boundary would make attack patterns visible in production logs without requiring trace queries.
4. **Token rotation path** — `load_admin_token` reads at startup only. A SIGHUP handler or `POST /v1/admin/reload-token` (protected by current token) would enable rotation without a restart.

---

*Generated by SERAPH + CORSO GUARD | platform-api-v1 Phase 4 | 2026-05-04*
