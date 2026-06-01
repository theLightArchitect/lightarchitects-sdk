//! HTTP handler for the active-containers list.
//!
//! - `GET /api/container/active` — returns a JSON array of running containers
//!   with their kind, policy snapshot, and age.

use std::time::Instant;

use axum::{Json, extract::State, response::IntoResponse};
use serde::Serialize;

use crate::{auth, container::types::ContainerKind, server::AppState};

/// Wire-format discriminated union for container kind.
///
/// Serialized with `#[serde(tag = "type")]` so the TypeScript side can
/// use a discriminated union on the `type` field.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ContainerKindView {
    /// Interactive PTY session — accepts WebSocket relay connections.
    Pty,
    /// Autonomous wave-task container — no WebSocket relay.
    WorkerTask {
        /// `IronClaw` task identifier.
        task_id: String,
        /// Wave index within the build (zero-based).
        wave_index: usize,
    },
}

impl ContainerKindView {
    fn from_kind(kind: &ContainerKind) -> Self {
        match kind {
            ContainerKind::Pty => Self::Pty,
            ContainerKind::WorkerTask {
                task_id,
                wave_index,
            } => Self::WorkerTask {
                task_id: task_id.clone(),
                wave_index: *wave_index,
            },
        }
    }
}

/// Actual hardening state applied at container startup.
///
/// Derived from `policy_snapshot_iso_mode`: Standard ⇒ Host userns;
/// Hardened / Airgapped ⇒ Remapped userns. `seccomp` and `cap_drop` are always
/// enabled because `build_container_run_args` applies them unconditionally.
#[derive(Debug, Serialize)]
pub struct HardeningActualView {
    /// `true` when a seccomp profile was applied.
    pub seccomp: bool,
    /// `true` when `--cap-drop ALL` was applied.
    pub cap_drop: bool,
    /// User-namespace remapping state.
    pub userns: &'static str,
}

impl HardeningActualView {
    fn from_iso_mode(mode: lightarchitects::container_spawn::IsoMode) -> Self {
        use lightarchitects::container_spawn::IsoMode;
        let userns = match mode {
            IsoMode::Standard => "Host",
            IsoMode::Hardened | IsoMode::Airgapped => "Remapped",
            _ => "Unsupported",
        };
        Self {
            seccomp: true,
            cap_drop: true,
            userns,
        }
    }
}

/// Wire-format view of a single active container entry.
#[derive(Debug, Serialize)]
pub struct ActiveContainerView {
    /// Docker container ID (full 64-char hex).
    pub container_id: String,
    /// Kind discriminant — `Pty` or `WorkerTask`.
    pub kind: ContainerKindView,
    /// `IsoMode` at spawn time as a label string.
    pub iso_mode_at_spawn: String,
    /// Network policy at spawn time (e.g. `"bridge"`, `"none"`).
    pub network_policy_at_spawn: String,
    /// Hardening state derived from `iso_mode` at spawn.
    pub hardening_actual: HardeningActualView,
    /// Seconds since `docker run` succeeded.
    pub age_secs: u64,
}

/// `GET /api/container/active` — returns the list of running containers.
///
/// Requires `Authorization: Bearer <token>` or `la_session` cookie.
/// Response is a JSON array sorted by `age_secs` ascending (newest last).
pub async fn get_active_containers(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let now = Instant::now();
    let mut views: Vec<ActiveContainerView> = if let Ok(g) = state.active_containers.read() {
        g.iter()
            .map(|(id, entry)| {
                let age_secs = now.saturating_duration_since(entry.started_at).as_secs();
                ActiveContainerView {
                    container_id: id.clone(),
                    kind: ContainerKindView::from_kind(&entry.kind),
                    iso_mode_at_spawn: entry.policy_snapshot_iso_mode.as_label_value().to_owned(),
                    network_policy_at_spawn: entry.network_policy_at_spawn.clone(),
                    hardening_actual: HardeningActualView::from_iso_mode(
                        entry.policy_snapshot_iso_mode,
                    ),
                    age_secs,
                }
            })
            .collect()
    } else {
        tracing::warn!(
            target: "container",
            "active_containers lock poisoned — returning empty list"
        );
        Vec::new()
    };
    // Stable sort: newest entries appear last (ascending age).
    views.sort_by_key(|v| v.age_secs);
    Json(views)
}
