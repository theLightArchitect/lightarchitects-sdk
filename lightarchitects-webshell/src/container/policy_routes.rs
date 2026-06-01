//! HTTP handlers for the live container spawn policy API.
//!
//! - `GET  /api/container/policy` — read the active policy snapshot.
//! - `PATCH /api/container/policy` — tighten the active policy (monotonic only).
//!
//! Policy updates are monotonic-tightening only (SERAPH constraint): the PATCH
//! handler calls `PolicyStore::tighten_for_build` which enforces that every
//! dimension of the new policy is at least as restrictive as the current one.
//!
//! # `ETag` / `If-Match`
//!
//! GET returns `ETag: "<N>"` where N is the current `AppState::policy_version`.
//! PATCH requires `If-Match: "<N>"` matching the current version; mismatches
//! return 412 Precondition Failed so the client can re-fetch and retry (G16 fix).
//!
//! # Rate limiting
//!
//! PATCH is limited to 1 call per second per Bearer token.  A second PATCH
//! within 1 s returns 429 Too Many Requests (G16 fix).

use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
    http::StatusCode,
    response::IntoResponse,
};
use lightarchitects::container_spawn::{
    AgentTier, ContainerPolicy, ContainerResources, IsoMode, NetworkPolicy, SpawnError, SpawnPolicy,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::{auth, server::AppState};

/// Wire-format snapshot of the active container policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyView {
    /// Isolation mode: `"standard"`, `"hardened"`, or `"airgapped"`.
    pub iso_mode: String,
    /// Network policy: `"bridge"`, `"host"`, `"none"`, or `"balanced"`.
    pub network: String,
    /// Memory cap in MiB.
    pub memory_mb: u64,
    /// CPU quota (fractional core count).
    pub cpus: f64,
    /// Process ID limit per container.
    pub pids_limit: u64,
    /// Maximum concurrent containers.
    pub max_concurrent: usize,
    /// Monotonic version counter, returned as `ETag` and used for `If-Match`.
    pub version: u64,
}

impl PolicyView {
    fn from_policy_and_version(p: &ContainerPolicy, version: u64) -> Self {
        Self {
            iso_mode: match p.iso_mode {
                IsoMode::Standard => "standard",
                IsoMode::Hardened => "hardened",
                IsoMode::Airgapped => "airgapped",
                _ => {
                    tracing::warn!(
                        target: "container.policy",
                        "PolicyView: unrecognized IsoMode variant — serializing as \"unknown\""
                    );
                    "unknown"
                }
            }
            .to_owned(),
            network: match p.network {
                NetworkPolicy::Bridge => "bridge",
                NetworkPolicy::Host => "host",
                NetworkPolicy::None => "none",
                NetworkPolicy::Balanced => "balanced",
                _ => {
                    tracing::warn!(
                        target: "container.policy",
                        "PolicyView: unrecognized NetworkPolicy variant — serializing as \"unknown\""
                    );
                    "unknown"
                }
            }
            .to_owned(),
            memory_mb: p.resources.memory_mb,
            cpus: p.resources.cpus,
            pids_limit: p.resources.pids_limit,
            max_concurrent: p.resources.max_concurrent,
            version,
        }
    }
}

/// `GET /api/container/policy` — returns the active policy snapshot.
///
/// Returns `ETag: "<version>"` so clients can use `If-Match` on PATCH.
///
/// Requires `Authorization: Bearer <token>` or `la_session` cookie.
pub async fn get_policy(_: auth::AuthGuard, State(state): State<AppState>) -> impl IntoResponse {
    let version = state.policy_version.load(Ordering::SeqCst);
    let policy = state.policy.load_full();
    let view = PolicyView::from_policy_and_version(policy.as_ref(), version);
    let etag = format!("\"{version}\"");
    (
        StatusCode::OK,
        [
            (axum::http::header::ETAG, etag),
            (axum::http::header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        Json(view),
    )
        .into_response()
}

/// Request body for `PATCH /api/container/policy`.
///
/// All fields are optional — omitted fields inherit from the current policy.
/// Accepted values may only tighten (reduce) the current limits.
#[derive(Debug, Deserialize)]
pub struct PatchPolicyRequest {
    /// New memory cap in MiB. Must be ≤ current cap.
    pub memory_mb: Option<u64>,
    /// New CPU quota. Must be ≤ current quota.
    pub cpus: Option<f64>,
    /// New pids limit. Must be ≤ current limit.
    pub pids_limit: Option<u64>,
    /// New concurrent cap. Must be ≤ current cap.
    pub max_concurrent: Option<usize>,
    /// New iso mode. Must be ≥ (at least as strict as) the current mode.
    ///
    /// Accepted values: `"standard"`, `"hardened"`, `"airgapped"`.
    pub iso_mode: Option<String>,
}

/// Parses a [`PatchPolicyRequest`] against the current policy and returns the
/// proposed [`ContainerPolicy`], or a rejection [`axum::response::Response`].
///
/// Extracted from `patch_policy` to keep that handler within the 100-line limit.
fn build_proposed_policy(
    req: &PatchPolicyRequest,
    current: &ContainerPolicy,
) -> Result<ContainerPolicy, Box<axum::response::Response>> {
    let iso_mode = if let Some(ref mode_str) = req.iso_mode {
        match mode_str.as_str() {
            "standard" => IsoMode::Standard,
            "hardened" => IsoMode::Hardened,
            "airgapped" => IsoMode::Airgapped,
            _ => {
                return Err(Box::new(
                    (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(serde_json::json!({
                            "error": format!("unknown iso_mode: {mode_str}")
                        })),
                    )
                        .into_response(),
                ));
            }
        }
    } else {
        current.iso_mode
    };

    // Airgapped mode requires NetworkPolicy::None.
    let network = if iso_mode == IsoMode::Airgapped {
        NetworkPolicy::None
    } else {
        current.network
    };

    let resources = ContainerResources {
        memory_mb: req.memory_mb.unwrap_or(current.resources.memory_mb),
        cpus: req.cpus.unwrap_or(current.resources.cpus),
        pids_limit: req.pids_limit.unwrap_or(current.resources.pids_limit),
        max_concurrent: req
            .max_concurrent
            .unwrap_or(current.resources.max_concurrent),
    };

    Ok(ContainerPolicy {
        iso_mode,
        network,
        resources,
        tier: AgentTier::Custom,
        ..ContainerPolicy::default()
    })
}

/// `PATCH /api/container/policy` — tighten the active policy.
///
/// Only monotonic tightening is accepted per the SERAPH constraint. Any field
/// that would loosen a limit returns `422 Unprocessable Entity`.
///
/// # Preconditions
///
/// - `If-Match: "<version>"` must match the current policy version → 412 on mismatch.
/// - Rate-limited to 1 PATCH/sec per Bearer token → 429 on burst.
///
/// Requires `Authorization: Bearer <token>` or `la_session` cookie.
pub async fn patch_policy(
    guard: auth::AuthGuard,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PatchPolicyRequest>,
) -> impl IntoResponse {
    // ── Rate limit: 1 PATCH/sec per token ────────────────────────────────────
    // AuthGuard already validated the token; re-extract just for hashing.
    let raw_auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token_hash = siphash(auth::extract_bearer(raw_auth).unwrap_or(""));
    let _ = guard; // token validated; guard no longer needed
    {
        let now = Instant::now();
        if let Some(last) = state.patch_rate_limiter.get(&token_hash) {
            if now.duration_since(*last) < Duration::from_secs(1) {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(axum::http::header::RETRY_AFTER, "1".to_owned())],
                    Json(serde_json::json!({ "error": "rate limit: 1 PATCH/sec per token" })),
                )
                    .into_response();
            }
        }
    }
    // Record this attempt regardless of outcome — failed PATCHes count against the rate limit.
    state.patch_rate_limiter.insert(token_hash, Instant::now());

    // ── CIDR guard: block requests from Docker bridge IPs (G4 + M1) ──────────
    if state.bridge_cidr_guard.is_blocked(peer.ip()) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "policy mutation from container network addresses is not permitted"
            })),
        )
            .into_response();
    }

    // ── If-Match version check (mandatory — 428 when absent) ─────────────────
    let current_version = state.policy_version.load(Ordering::SeqCst);
    let Some(if_match_header) = headers.get(axum::http::header::IF_MATCH) else {
        return (
            StatusCode::PRECONDITION_REQUIRED,
            Json(serde_json::json!({
                "error": "If-Match header required — fetch GET /api/container/policy for the current ETag",
            })),
        )
            .into_response();
    };
    let expected = format!("\"{current_version}\"");
    if if_match_header.to_str().unwrap_or("") != expected {
        return (
            StatusCode::PRECONDITION_FAILED,
            Json(serde_json::json!({
                "error": "If-Match mismatch — re-fetch GET /api/container/policy and retry",
                "current_version": current_version,
            })),
        )
            .into_response();
    }

    // ── Apply policy ──────────────────────────────────────────────────────────
    let current = state.policy.load_full();
    let proposed = match build_proposed_policy(&req, &current) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    // tighten_for_build validates + enforces the monotonic-tightening invariant.
    match state.policy_store.tighten_for_build(&proposed) {
        Ok(tightened) => {
            if let Err(e) = state
                .policy_store
                .update_system_policy((*tightened).clone())
            {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }

            // Increment version counter.
            let new_version = state.policy_version.fetch_add(1, Ordering::SeqCst) + 1;

            let etag = format!("\"{new_version}\"");
            (
                StatusCode::OK,
                [(axum::http::header::ETAG, etag)],
                Json(PolicyView::from_policy_and_version(
                    tightened.as_ref(),
                    new_version,
                )),
            )
                .into_response()
        }
        Err(SpawnError::PolicyTighteningViolation(msg)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Non-cryptographic hash of a string for rate-limit bucketing.
///
/// Uses `DefaultHasher` (SipHash-1-3 variant), which is **not stable** across
/// Rust versions or process restarts — suitable only for ephemeral, in-process
/// state such as this rate-limit map.
fn siphash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}
