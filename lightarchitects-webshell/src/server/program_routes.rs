//! `GET /api/program-manifest` — serves the alpha readiness program manifest as JSON.
//!
//! Reads `~/lightarchitects/soul/helix/program/alpha-readiness/program_manifest.yaml`,
//! parses it as a dynamic `serde_yaml::Value`, and returns it as JSON. Returns `404`
//! when the file is absent and `500` on YAML parse errors.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde_yaml::Value;

use crate::{auth, server::AppState};

/// Serves the alpha program manifest as JSON.
///
/// * `200` — manifest parsed and returned as JSON.
/// * `404` — `program_manifest.yaml` not found on disk.
/// * `500` — YAML parse error.
pub async fn program_manifest_handler(
    _: auth::AuthGuard,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let path = dirs_next::home_dir()
        .map(|h| h.join("lightarchitects/soul/helix/program/alpha-readiness/program_manifest.yaml"))
        .unwrap_or_default();

    match tokio::fs::read_to_string(&path).await {
        Ok(yaml) => match serde_yaml::from_str::<Value>(&yaml) {
            Ok(val) => Json(val).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("program_manifest parse error: {e}"),
            )
                .into_response(),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (StatusCode::NOT_FOUND, "program_manifest.yaml not found").into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("program_manifest read error: {e}"),
        )
            .into_response(),
    }
}
