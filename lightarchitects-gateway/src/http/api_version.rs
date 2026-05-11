//! API contract-surface versioning (OD-6).
//!
//! `API_VERSION_HASH` is a deterministic SHA-256 fingerprint of the admin +
//! platform route surface, computed from:
//! - `ALLOWED_KINDS` and `ALLOWED_SIBLINGS` (sorted)
//! - All admin mutation signatures (`METHOD PATH`, sorted)
//! - All platform read signatures (`METHOD PATH`, sorted)
//!
//! The hash is baked as a compile-time constant. A failing `api_version_test.rs`
//! integration test is the change-control gate: any drift in the contract surface
//! (new route, renamed slug, added allowed value) must be accompanied by an
//! intentional hash update and a changelog entry.
//!
//! # Compute / update the hash
//!
//! ```bash
//! # With temp workspace (see CLAUDE.md gateway-build-workaround):
//! cargo test -p lightarchitects-gateway --test api_version_test -- \
//!     --nocapture 2>&1 | grep 'api_version_hash ='
//! # Then paste the printed value into API_VERSION_HASH below.
//! ```

use crate::http::etag::sha256_hex;
use crate::http::routes::admin::{ALLOWED_KINDS, ALLOWED_SIBLINGS};

/// ISO date when this hash revision was cut.
pub const API_VERSION_DATE: &str = "2026-05-10";

/// SHA-256 fingerprint (first 16 hex chars) of the admin + platform contract surface.
///
/// Update via `cargo test -p lightarchitects-gateway --test api_version_test -- --nocapture`.
/// Any PR that changes this constant is a BREAKING CHANGE and requires changelog entry.
pub const API_VERSION_HASH: &str = "2ce8dae12af41c12";

/// Number of route signatures included in the fingerprint (admin + platform combined).
pub const CONTRACT_SURFACE_COUNT: usize = 23;

/// Compute a deterministic fingerprint of the admin + platform API contract surface.
///
/// Inputs are sorted before hashing to guarantee stability across compilation targets.
/// First 16 hex characters of the SHA-256 digest are returned (64-bit prefix — sufficient
/// for drift detection; collision probability at this length is negligible for a bounded
/// route set).
pub fn compute_api_version_hash() -> String {
    // Admin mutation surface (7 routes).
    let admin_routes = [
        "DELETE /v1/admin/overrides/{org_id}/{*target_path}",
        "POST /v1/admin/agents/upload",
        "POST /v1/admin/canon/upload",
        "POST /v1/admin/operator/resolve-assertion",
        "POST /v1/admin/overrides",
        "POST /v1/admin/skills/upload",
        "POST /v1/admin/standards/upload",
    ];

    // Platform read surface (16 routes — includes /v1/version and POST helix/search).
    let platform_routes = [
        "GET /v1/identity",
        "GET /v1/platform/agents/{sibling}",
        "GET /v1/platform/agents/{sibling}/strands",
        "GET /v1/platform/canon/{name}",
        "GET /v1/platform/health",
        "GET /v1/platform/helix/query",
        "GET /v1/platform/helix/search",
        "GET /v1/platform/helix/stream",
        "GET /v1/platform/personas",
        "GET /v1/platform/personas/{name}",
        "GET /v1/platform/skills",
        "GET /v1/platform/skills/{name}",
        "GET /v1/platform/standards/{name}",
        "GET /v1/vault/info",
        "GET /v1/version",
        "POST /v1/platform/helix/search",
    ];

    // Build sorted canonical input list.
    let mut parts: Vec<&str> = Vec::new();

    // Allowlist enums — sorted copies (source slices may not be sorted).
    let mut kinds = ALLOWED_KINDS.to_vec();
    kinds.sort_unstable();
    parts.extend_from_slice(&kinds);

    let mut siblings = ALLOWED_SIBLINGS.to_vec();
    siblings.sort_unstable();
    parts.extend_from_slice(&siblings);

    // Route signatures — merge and sort.
    let mut routes: Vec<&str> = admin_routes
        .iter()
        .chain(platform_routes.iter())
        .copied()
        .collect();
    routes.sort_unstable();
    parts.extend_from_slice(&routes);

    let canonical = parts.join("\n");
    sha256_hex(canonical.as_bytes())[..16].to_owned()
}
