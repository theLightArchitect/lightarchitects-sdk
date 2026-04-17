//! `GET /api/builds` — returns the parsed `active.yaml` build tracking data.
//!
//! Reads `~/.soul/helix/corso/builds/active.yaml` from disk and returns it
//! as JSON. Returns 503 if the vault is not configured or the file is missing.
//!
//! The handler caches the file content by mtime: if the file hasn't changed
//! since the last read, the cached JSON is returned directly without
//! re-parsing. This avoids redundant YAML parsing on every request.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::{extract::State, http::StatusCode, response::IntoResponse};
use tracing::{info, warn};

use crate::{auth, server::AppState};

/// Cached build data: (mtime, serialised JSON bytes).
pub type Cache = Arc<Mutex<Option<(SystemTime, Vec<u8>)>>>;

/// Shared cache instance, created once per server lifetime.
#[must_use]
pub fn build_cache() -> Cache {
    Arc::new(Mutex::new(None))
}

/// `GET /api/builds` — returns build tracking data as JSON.
///
/// Auth-gated (same Bearer token as `/api/events`).
/// Returns 503 if the vault is not configured or the file doesn't exist.
#[allow(clippy::missing_panics_doc)]
pub async fn builds_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Validate bearer token.
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Resolve the active.yaml path.
    let Some(helix_root) = lightarchitects_core::paths::helix_root() else {
        warn!("helix_root unavailable — cannot serve /api/builds");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let path = helix_root.join("corso").join("builds").join("active.yaml");

    let metadata = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "active.yaml not found");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };

    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    // Check the cache — if mtime matches, return cached bytes.
    // Mutex lock is held briefly; a poisoned lock from a panic is acceptable
    // because the server would be in an inconsistent state anyway.
    #[allow(clippy::unwrap_used)]
    let cache_hit = {
        let cache = state.builds_cache.lock().unwrap();
        cache.as_ref().and_then(|(cached_mtime, cached_bytes)| {
            if *cached_mtime == mtime {
                Some(cached_bytes.clone())
            } else {
                None
            }
        })
    };

    if let Some(cached_bytes) = cache_hit {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            cached_bytes,
        )
            .into_response();
    }

    // Read and parse the YAML file.
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "failed to read active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to parse active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Convert YAML → JSON for the browser.
    let json_value = serde_json::to_value(&yaml_value).unwrap_or_else(|e| {
        warn!(error = %e, "failed to convert YAML to JSON");
        serde_json::Value::Null
    });

    let json_bytes = match serde_json::to_vec_pretty(&json_value) {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "failed to serialise builds JSON");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    info!(path = %path.display(), "served /api/builds");

    // Update cache.
    #[allow(clippy::unwrap_used)]
    {
        *state.builds_cache.lock().unwrap() = Some((mtime, json_bytes.clone()));
    }

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json_bytes,
    )
        .into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn build_cache_initialises_empty() {
        let cache = build_cache();
        assert!(cache.lock().unwrap().is_none());
    }
}
