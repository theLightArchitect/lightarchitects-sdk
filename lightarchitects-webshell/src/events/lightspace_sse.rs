//! `GET /api/lightspace/:session_id/events` — per-session SSE fan-out.
//!
//! Streams [`WebEventV2`] events whose `topic` starts with `v1.lightspace.`.
//!
//! ## Auth
//!
//! Requires `Authorization: Bearer <token>`.  Returns 401 on failure.
//!
//! ## Subscribe-before-dispatch invariant
//!
//! [`broadcast::Receiver`] is created by `slot.broadcast_tx.subscribe()`
//! before the `Sse` stream is returned — events emitted between stream
//! construction and the first `.recv()` poll are buffered in the channel
//! ring and delivered in order (subscribe BEFORE dispatch, CWE-662).
//!
//! ## Lag handling
//!
//! On [`RecvError::Lagged`] a synthetic `{"type":"lag","skipped":N}` event is
//! emitted and the stream continues (OWASP LLM01 — drop-and-warn, not crash).

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use tokio::sync::broadcast::error::RecvError;
use tracing::{instrument, warn};
use uuid::Uuid;

use crate::{auth, server::AppState};

/// SSE stream for a single Lightspace session.
///
/// Moves the broadcast [`Receiver`] into `stream::unfold` state so it can
/// be held across `await` points without violating the `FnMut` borrow rules.
///
/// [`Receiver`]: tokio::sync::broadcast::Receiver
#[instrument(name = "lightspace.sse", skip_all, fields(session_id = %session_id))]
pub async fn lightspace_sse_handler(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !auth::validate_bearer(auth_header, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Establish the slot (and its broadcast channel) before subscribing —
    // satisfies the subscribe-before-dispatch invariant.
    let slot = state.lightspace_registry.get_or_create(session_id);

    // Subscribe BEFORE returning the stream.  Move `rx` into unfold state so
    // it can be owned across `.await` inside the async block.
    let rx = slot.broadcast_tx.subscribe();

    let stream = stream::unfold(rx, move |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(envelope) => {
                    if !envelope.topic.starts_with("v1.lightspace.") {
                        continue;
                    }
                    match serde_json::to_string(&envelope) {
                        Ok(json) => {
                            return Some((
                                Ok::<Event, Infallible>(Event::default().data(json)),
                                rx,
                            ));
                        }
                        Err(e) => {
                            warn!(error = %e, "lightspace SSE: serialize failed — skipping event");
                            // loop continues: next recv() call on same rx
                        }
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    warn!(
                        skipped = n,
                        %session_id,
                        "lightspace SSE subscriber lagged — events dropped"
                    );
                    let payload = format!(r#"{{"type":"lag","skipped":{n}}}"#);
                    return Some((Ok::<Event, Infallible>(Event::default().data(payload)), rx));
                }
                Err(RecvError::Closed) => {
                    return None;
                }
            }
        }
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
