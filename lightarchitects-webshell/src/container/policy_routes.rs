//! HTTP handlers for the live container spawn policy API.
//!
//! - `GET  /api/container/policy` — read the active policy snapshot.
//! - `PATCH /api/container/policy` — tighten the active policy (monotonic only).
//!
//! Policy updates are monotonic-tightening only (SERAPH constraint): the PATCH
//! handler calls `PolicyStore::tighten_for_build` which enforces that every
//! dimension of the new policy is at least as restrictive as the current one.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use lightarchitects::container_spawn::{
    AgentTier, ContainerPolicy, ContainerResources, IsoMode, NetworkPolicy, SpawnError, SpawnPolicy,
};
use serde::{Deserialize, Serialize};

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
}

impl From<&ContainerPolicy> for PolicyView {
    fn from(p: &ContainerPolicy) -> Self {
        Self {
            iso_mode: match p.iso_mode {
                IsoMode::Standard => "standard",
                IsoMode::Hardened => "hardened",
                IsoMode::Airgapped => "airgapped",
                _ => "unknown",
            }
            .to_owned(),
            network: match p.network {
                NetworkPolicy::Bridge => "bridge",
                NetworkPolicy::Host => "host",
                NetworkPolicy::None => "none",
                NetworkPolicy::Balanced => "balanced",
                _ => "unknown",
            }
            .to_owned(),
            memory_mb: p.resources.memory_mb,
            cpus: p.resources.cpus,
            pids_limit: p.resources.pids_limit,
            max_concurrent: p.resources.max_concurrent,
        }
    }
}

/// `GET /api/container/policy` — returns the active policy snapshot.
///
/// Requires `Authorization: Bearer <token>` or `la_session` cookie.
pub async fn get_policy(_: auth::AuthGuard, State(state): State<AppState>) -> impl IntoResponse {
    let policy = state.policy.load_full();
    (StatusCode::OK, Json(PolicyView::from(policy.as_ref())))
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

/// `PATCH /api/container/policy` — tighten the active policy.
///
/// Only monotonic tightening is accepted per the SERAPH constraint. Any field
/// that would loosen a limit returns `422 Unprocessable Entity`.
///
/// Requires `Authorization: Bearer <token>` or `la_session` cookie.
pub async fn patch_policy(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    Json(req): Json<PatchPolicyRequest>,
) -> impl IntoResponse {
    let current = state.policy.load_full();

    let iso_mode = if let Some(ref mode_str) = req.iso_mode {
        match mode_str.as_str() {
            "standard" => IsoMode::Standard,
            "hardened" => IsoMode::Hardened,
            "airgapped" => IsoMode::Airgapped,
            _ => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "error": format!("unknown iso_mode: {mode_str}")
                    })),
                )
                    .into_response();
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

    let proposed = ContainerPolicy {
        iso_mode,
        network,
        resources,
        tier: AgentTier::Custom,
        ..ContainerPolicy::default()
    };

    // tighten_for_build validates + enforces the monotonic-tightening invariant.
    match state.policy_store.tighten_for_build(&proposed) {
        Ok(tightened) => {
            // Persist the tightened policy as the new system baseline.
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
            (StatusCode::OK, Json(PolicyView::from(tightened.as_ref()))).into_response()
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
