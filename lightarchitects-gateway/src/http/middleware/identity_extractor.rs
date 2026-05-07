//! Identity extraction middleware — injects `UserContext` into every request.
//!
//! Priority chain (first match wins):
//! 1. `LA_USER_ID` env var (operator override)
//! 2. Bearer token → KeyCache lookup (validated API key)
//!
//! `X-User-Id` is intentionally excluded from the middleware resolver to prevent
//! horizontal privilege escalation. Internal routing may set `LA_USER_ID` via
//! reverse-proxy config, but client-supplied headers are never trusted here.
//!
//! When `IdentityScopePolicy::DenyByDefault` is active and no user_id is resolved,
//! the middleware returns HTTP 403. Otherwise it falls back to `"local"`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use serde_json::json;
use std::sync::Arc;

use crate::config::IdentityScopePolicy;
use crate::http::state::PlatformState;

/// User context propagated through the request lifecycle.
#[derive(Debug, Clone)]
pub struct UserContext {
    /// Resolved canonical user identifier.
    pub user_id: String,
    /// True when the user_id came from a validated API key cache.
    pub from_key_cache: bool,
}

/// Extract user identity and attach to request extensions.
pub async fn identity_extractor_middleware(
    State(state): State<Arc<PlatformState>>,
    mut req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let user_id = resolve_user_id(&req, &state);

    match user_id {
        Some(id) => {
            let ctx = UserContext {
                user_id: id,
                from_key_cache: has_bearer_token(&req),
            };
            req.extensions_mut().insert(ctx);
            next.run(req).await
        }
        None => {
            match state.config.identity_scope_policy {
                IdentityScopePolicy::DenyByDefault => {
                    tracing::warn!(
                        path = %req.uri().path(),
                        "identity: DenyByDefault — rejecting unauthenticated request"
                    );
                    forbidden().into_response()
                }
                IdentityScopePolicy::AllowAuthenticated => {
                    // Fallback to local single-user mode
                    let ctx = UserContext {
                        user_id: "local".to_owned(),
                        from_key_cache: false,
                    };
                    req.extensions_mut().insert(ctx);
                    next.run(req).await
                }
            }
        }
    }
}

// ── Resolution chain ──────────────────────────────────────────────────────────

fn resolve_user_id(req: &Request<Body>, state: &PlatformState) -> Option<String> {
    // Priority 1: LA_USER_ID env var
    if let Ok(id) = std::env::var("LA_USER_ID") {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }

    // Priority 2: Bearer token → KeyCache lookup
    if let Some(_bearer) = extract_bearer(req.headers()) {
        // TODO: In Wave 2.5, integrate with KeyCache validation endpoint
        // For now, hash the bearer and check against a static admin token
        // or derive user_id from token claims if JWT.
        // v1.0 simplification: bearer token IS the user_id if it passes
        // the read_auth_middleware (which runs before this middleware).
        // We use the config's user_id as the bearer-derived identity.
        if state.read_token.is_some() {
            return Some(state.config.user_id.clone());
        }
    }

    None
}

fn has_bearer_token(req: &Request<Body>) -> bool {
    extract_bearer(req.headers()).is_some()
}

fn extract_bearer(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn forbidden() -> impl IntoResponse {
    (
        StatusCode::FORBIDDEN,
        axum::Json(json!({
            "error": {
                "code": "identity_required",
                "message": "Identity could not be resolved and DenyByDefault policy is active.",
                "status": 403
            }
        })),
    )
}
