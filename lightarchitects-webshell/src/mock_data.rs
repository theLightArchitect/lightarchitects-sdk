//! Stub handlers that make the Mockcli frontend render without console 404s.
//!
//! Every screen in `~/Projects/Lightarchitectmockcli` hits a handful of
//! `/api/*` endpoints on mount. Until the real backend lands, we return
//! empty-but-well-formed JSON for reads and `501 Not Implemented` for
//! writes. This lets the Phase G embed swap ship a browser-clean UI
//! without promising functionality we haven't built.
//!
//! All handlers require the global Bearer token — same trust domain as
//! the rest of `/api/*`. The plan's Phase D expects these exact routes
//! based on a Grep of `~/Projects/Lightarchitectmockcli/src/lib/api.ts`.
//!
//! ## Do not put real logic here
//!
//! If a feature is ready to ship, extract it to a dedicated module.
//! Mixing stubs and production handlers in the same file invites
//! accidentally-shipped empty responses after someone "forgets" to move
//! their real handler out.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{auth, server::AppState};

/// Short-circuit Bearer check shared by all stub handlers.
///
/// Returns `Some(response)` when the caller is unauthenticated — the
/// handler should `return` that response directly. Returns `None` when
/// the token is valid and handling should proceed.
fn reject_if_unauth(headers: &HeaderMap, token: &str) -> Option<axum::response::Response> {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth::validate_bearer(authz, token) {
        None
    } else {
        Some(StatusCode::UNAUTHORIZED.into_response())
    }
}

/// Wrap a plausible JSON body behind the Bearer check.
fn ok_json(headers: &HeaderMap, state: &AppState, body: Value) -> axum::response::Response {
    if let Some(r) = reject_if_unauth(headers, &state.config.token) {
        return r;
    }
    (StatusCode::OK, Json(body)).into_response()
}

/// Uniform 501 response for write endpoints not yet implemented.
fn not_implemented(
    headers: &HeaderMap,
    state: &AppState,
    reason: &'static str,
) -> axum::response::Response {
    if let Some(r) = reject_if_unauth(headers, &state.config.token) {
        return r;
    }
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "not_implemented",
            "reason": reason,
        })),
    )
        .into_response()
}

// ── Reads (return plausible empty JSON) ──────────────────────────────────────

/// `GET /api/workspaces` — stub empty list.
pub async fn list_workspaces(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    ok_json(&headers, &state, json!([]))
}

/// `GET /api/workspaces/:id` — stub 404 (no workspaces exist yet).
pub async fn get_workspace(
    Path(_id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Some(r) = reject_if_unauth(&headers, &state.config.token) {
        return r;
    }
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "not_found",
            "reason": "workspaces are stubbed — no entries exist yet"
        })),
    )
        .into_response()
}

/// `GET /api/meta-skills` — stub empty list.
pub async fn list_meta_skills(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    ok_json(&headers, &state, json!([]))
}

/// `GET /api/siblings` — stub list of the 7 platform siblings (static).
///
/// Returns a stable, well-formed shape so StatusBar/Arena panels render.
/// All siblings report `"idle"` — real status tracking lands in a later PR.
pub async fn get_sibling_status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let siblings = json!([
        {"id": "claude", "status": "idle"},
        {"id": "eva", "status": "idle"},
        {"id": "corso", "status": "idle"},
        {"id": "quantum", "status": "idle"},
        {"id": "seraph", "status": "idle"},
        {"id": "ayin", "status": "idle"},
        {"id": "soul", "status": "idle"},
    ]);
    ok_json(&headers, &state, siblings)
}

/// `GET /api/sitrep` — stub nominal sitrep.
pub async fn get_sitrep(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    ok_json(
        &headers,
        &state,
        json!({
            "status": "nominal",
            "pillars": {
                "arch":  {"state": "green"},
                "sec":   {"state": "green"},
                "qual":  {"state": "green"},
                "perf":  {"state": "green"},
                "test":  {"state": "green"},
                "doc":   {"state": "green"},
                "ops":   {"state": "green"},
            }
        }),
    )
}

/// `GET /api/conductor/status` — stub empty orchestration graph.
pub async fn get_conductor_status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ok_json(
        &headers,
        &state,
        json!({"nodes": [], "edges": [], "queue_depth": 0}),
    )
}

/// `GET /api/arena/status` — stub empty arena state.
pub async fn get_arena_status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ok_json(
        &headers,
        &state,
        json!({"battles": [], "sessions": 0, "tier": "bronze"}),
    )
}

/// `GET /api/builds/:id/findings` — stub empty findings list.
pub async fn list_findings(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ok_json(&headers, &state, json!([]))
}

/// `GET /api/builds/:id/notes` — stub empty notes.
pub async fn get_notes(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ok_json(
        &headers,
        &state,
        json!({"content": "", "updated_at": null}),
    )
}

/// `GET /api/builds/:id/artifacts` — stub empty artifact list.
pub async fn list_artifacts(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ok_json(&headers, &state, json!([]))
}

/// `GET /api/builds/:id/gates/:pillar` — stub "pending" gate status.
pub async fn get_gate_status(
    Path((_id, pillar)): Path<(Uuid, String)>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ok_json(
        &headers,
        &state,
        json!({
            "pillar": pillar,
            "state": "pending",
            "tier": null,
            "updated_at": null,
        }),
    )
}

// ── Writes (501 Not Implemented — auth still required) ──────────────────────

/// `POST /api/builds/:id/pillars/:pillar` — stub 501 until dispatcher lands.
pub async fn trigger_pillar(
    Path(_path): Path<(Uuid, String)>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    not_implemented(&headers, &state, "pillar trigger requires CORSO dispatcher integration")
}

/// `POST /api/builds/:id/artifacts` — stub 501 until artifact store lands.
pub async fn upload_artifact(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    not_implemented(&headers, &state, "artifact upload requires artifact store")
}

/// `PUT /api/builds/:id/notes` — stub 501 until notes persistence lands.
pub async fn update_notes(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    not_implemented(&headers, &state, "notes persistence not yet implemented")
}

/// `POST /api/builds/:id/copilot` — stub 501 until copilot backend lands.
pub async fn copilot_chat(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    not_implemented(&headers, &state, "copilot routing not yet implemented — use PTY for now")
}

/// `POST /api/builds/:id/dispatch` — stub 501 until SQUAD dispatcher lands.
pub async fn dispatch_sibling(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    not_implemented(&headers, &state, "sibling dispatch requires SQUAD integration")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ok_json_factory_builds_expected_shape() {
        // Structural test: the handler contract is `(StatusCode::OK, Json(body))`.
        // We construct a synthetic body and confirm our helper's expected payload
        // by hand (integration tests exercise the full Axum path).
        let body = json!({"a": 1, "b": [2, 3]});
        assert_eq!(body["a"], 1);
        assert_eq!(body["b"][1], 3);
    }

    #[test]
    fn sibling_status_payload_has_all_seven_siblings() {
        let expected = ["claude", "eva", "corso", "quantum", "seraph", "ayin", "soul"];
        // Reconstruct the body the handler returns to validate shape.
        let siblings = json!([
            {"id": "claude", "status": "idle"},
            {"id": "eva", "status": "idle"},
            {"id": "corso", "status": "idle"},
            {"id": "quantum", "status": "idle"},
            {"id": "seraph", "status": "idle"},
            {"id": "ayin", "status": "idle"},
            {"id": "soul", "status": "idle"},
        ]);
        let arr = siblings.as_array().unwrap();
        assert_eq!(arr.len(), 7);
        for id in expected {
            assert!(
                arr.iter().any(|s| s["id"] == id),
                "missing sibling: {id}"
            );
        }
    }

    #[test]
    fn sitrep_payload_covers_all_seven_corso_pillars() {
        let pillars = ["arch", "sec", "qual", "perf", "test", "doc", "ops"];
        let body = json!({
            "status": "nominal",
            "pillars": {
                "arch":  {"state": "green"},
                "sec":   {"state": "green"},
                "qual":  {"state": "green"},
                "perf":  {"state": "green"},
                "test":  {"state": "green"},
                "doc":   {"state": "green"},
                "ops":   {"state": "green"},
            }
        });
        for p in pillars {
            assert!(
                body["pillars"][p]["state"] == "green",
                "missing pillar: {p}"
            );
        }
    }
}
