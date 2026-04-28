//! Axum server: app construction, shared state, routes, run loop.
//!
//! Phase 1 wires three concerns: a liveness probe, an auth-check endpoint
//! that exercises the HMAC comparator, and a rust-embed static-asset
//! fallback serving the frontend bundle.
//!
//! Phase 2 adds `/api/terminal/ws` (PTY WebSocket bridge).
//! Phase 3/5 will add `/api/events` (SSE fan-out).

use std::{
    net::SocketAddr,
    sync::{Arc, atomic::AtomicUsize},
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use tokio::sync::{RwLock, broadcast};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use crate::{
    auth,
    config::{AgentSession, Config},
    copilot,
    events::{self, EVENT_CHANNEL_BUF, WebEvent, builds_handler},
    polytope_data, real_data,
    session::BuildRegistry,
    session_fork, setup, static_assets, terminal,
};

/// Snapshot of the browser UI state, periodically reported by the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserStateSnapshot {
    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,
    /// Terminal panel size in percent.
    pub terminal_size_percent: u8,
    /// Helix panel size in percent.
    pub helix_size_percent: u8,
    /// Currently focused panel identifier.
    pub active_panel: String,
    /// Helix 3D scene zoom level.
    pub helix_zoom: f32,
    /// Number of visible helix steps.
    pub helix_step_count: usize,
}

impl Default for BrowserStateSnapshot {
    fn default() -> Self {
        Self {
            viewport_width: 0,
            viewport_height: 0,
            terminal_size_percent: 50,
            helix_size_percent: 50,
            active_panel: String::from("terminal"),
            helix_zoom: 5.0,
            helix_step_count: 0,
        }
    }
}

/// Shared application state threaded into every handler.
///
/// Cloning is cheap — both inner values live behind [`Arc`]s, and
/// [`broadcast::Sender`] is itself a cheaply-clonable handle.
#[derive(Clone)]
pub struct AppState {
    /// Resolved config: port, `host_cmd`, cwd, token.
    pub config: Arc<Config>,
    /// Turnlog pepper — loaded once at startup from session key.
    pub turnlog_pepper: Arc<secrecy::SecretSlice<u8>>,
    /// Number of active PTY sessions (max [`terminal::ws::MAX_SESSIONS`]).
    pub session_count: Arc<AtomicUsize>,
    /// Broadcast sender for internal [`WebEvent`]s.
    ///
    /// The Phase-5 SSE handler calls [`broadcast::Sender::subscribe`] on
    /// this to obtain a per-connection [`broadcast::Receiver`].
    pub event_tx: broadcast::Sender<WebEvent>,
    /// Latest browser UI state, updated periodically by the frontend.
    pub browser_state: Arc<RwLock<BrowserStateSnapshot>>,
    /// Cached build tracking data (active.yaml mtime + JSON bytes).
    pub builds_cache: events::builds_handler::Cache,
    /// Per-build session registry (Phase C).
    ///
    /// Keys are UUIDs minted at `POST /api/builds`; values are
    /// `Arc<BuildSession>` clones returned by [`BuildRegistry::get`] —
    /// safe to hold across `.await` (no `DashMap` ref guard escapes).
    pub builds: Arc<BuildRegistry>,
    /// Active agent session — updated live by `POST /api/setup/save`.
    ///
    /// Initially set to `config.agent`; all new [`BuildSession`]s read this
    /// field so backend switches take effect immediately without restarting
    /// the server.  Existing sessions keep their original [`AgentSession`].
    pub active_agent: Arc<RwLock<AgentSession>>,
    /// `SOUL` persistence handle — Phase 10.1. Opened at startup against
    /// `~/lightarchitects/soul/helix.db` (`SQLite` backend). `None` when the
    /// file doesn't exist or fails to open; the webshell degrades to
    /// filesystem-only reads in that case.
    ///
    /// This is the **same backend** the `SOUL` `MCP` plugin writes through, so
    /// entries ingested from Claude Code are immediately visible here without
    /// an extra fetch layer.
    pub soul_store: Option<Arc<crate::memory::persistence::SoulPersistence>>,
    /// Phase 19c.2 — hot-reloadable promotion policy handle.
    ///
    /// `None` when the policy YAML could not be resolved (e.g. `HOME` unavailable).
    /// Threads through to `WebshellTurnLog::with_policy` at session open so
    /// promotion floors stay current without a server restart.
    pub promotion_policy: Option<lightarchitects::turnlog::PolicyHandle>,
    /// Keeps the `notify` file-system subscription alive for the lifetime of the
    /// server.  Intentionally not exposed — callers read through `promotion_policy`.
    /// Wrapped in `Arc` so `AppState` can derive `Clone` (watcher itself is not `Clone`).
    _policy_watcher: Option<std::sync::Arc<lightarchitects::turnlog::PolicyWatcher>>,
    /// Phase 17b — lazily-initialised embedding provider used by the
    /// semantic/hybrid search paths. First call from `search_handler`
    /// boots `FastEmbedProvider::try_new(Default)` on the blocking pool;
    /// subsequent calls reuse the cached Arc. Initialization failure
    /// silently degrades to [`MockEmbeddingProvider`] — the search path
    /// stays available even if ONNX or the cache directory is broken.
    pub embedding_provider: Arc<
        tokio::sync::OnceCell<
            Arc<dyn lightarchitects::soul::embedding::EmbeddingProvider + Send + Sync>,
        >,
    >,
}

impl AppState {
    /// Constructs a new state from a resolved [`Config`] and spawns the
    /// background AYIN SSE subscription task.
    #[must_use]
    pub fn new(config: Config) -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_BUF);
        events::AyinClient::spawn(event_tx.clone());
        events::HelixWatcher::spawn(event_tx.clone());
        let pepper = load_turnlog_pepper();
        let active_agent = Arc::new(RwLock::new(config.agent.clone()));
        let soul_store = Some(Arc::new(crate::memory::persistence::SoulPersistence::open()));

        // Phase 19c.2 — hot-reloadable promotion policy.
        // Gracefully absent when the YAML file doesn't exist yet.
        let (promotion_policy, policy_watcher) =
            lightarchitects::turnlog::PolicyWatcher::default_path().map_or((None, None), |p| {
                let (h, w) = lightarchitects::turnlog::PolicyWatcher::spawn(&p);
                (Some(h), Some(std::sync::Arc::new(w)))
            });

        // Phase 11.1 — auto-backfill SQLite from filesystem on startup.
        // Phase 11.3 — attach Neo4j if WEBSHELL_NEO4J_URI is set + reachable.
        // Both are fire-and-forget so server boot doesn't wait on I/O.
        if let Some(soul) = soul_store.clone() {
            let soul_for_backfill = soul.clone();
            tokio::spawn(async move {
                let Some(backend) = soul_for_backfill.sqlite_arc() else {
                    return;
                };
                if crate::memory::backfill::sqlite_needs_backfill(
                    &backend,
                    soul_for_backfill.helix_root(),
                )
                .await
                {
                    let report =
                        crate::memory::backfill::run(soul_for_backfill.helix_root(), &backend)
                            .await;
                    tracing::info!(
                        target: "soul",
                        scanned = report.scanned,
                        written = report.written,
                        "startup backfill complete"
                    );
                }
            });
            // Phase 11.3 — attach Neo4j first (populator needs it).
            // Phase 17b — spawn boot-time embedding populator AFTER the
            // Neo4j attach completes, so the populator's `soul.neo4j_arc()`
            // lookup succeeds on the fast path.
            let soul_for_embed = soul.clone();
            let tx_for_convergence = event_tx.clone();
            tokio::spawn(async move {
                soul_for_embed.clone().try_attach_neo4j().await;
                crate::memory::embedder::spawn(soul_for_embed.clone());
                // Phase 19b.2 — cross-sibling strand convergence detector.
                // Runs in parallel with the embedder; both poll the graph
                // on their own cadences (embedder = one-shot at boot;
                // convergence = every 60s).
                crate::memory::convergence::spawn(soul_for_embed, tx_for_convergence);
            });
        }
        Self {
            config: Arc::new(config),
            turnlog_pepper: Arc::new(pepper),
            session_count: Arc::new(AtomicUsize::new(0)),
            event_tx,
            browser_state: Arc::new(RwLock::new(BrowserStateSnapshot::default())),
            builds_cache: builds_handler::build_cache(),
            builds: Arc::new(BuildRegistry::new()),
            active_agent,
            soul_store,
            promotion_policy,
            _policy_watcher: policy_watcher,
            embedding_provider: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Phase 17b — lazily-initialised embedding provider for the search path.
    ///
    /// First call spawns `FastEmbedProvider::try_new(Default)` on the tokio
    /// blocking pool (~4s cold start). On success the Arc is cached in
    /// [`Self::embedding_provider`] and reused for subsequent queries. On
    /// failure (no cache writable, ONNX missing, download refused) the
    /// error is logged once and a [`MockEmbeddingProvider`] is cached in
    /// its place so the semantic path remains operational.
    pub async fn embedding(
        &self,
    ) -> Arc<dyn lightarchitects::soul::embedding::EmbeddingProvider + Send + Sync> {
        use lightarchitects::soul::embedding::{
            EmbeddingProvider,
            fastembed::{FastEmbedModel, FastEmbedProvider},
            mock::MockEmbeddingProvider,
        };
        self.embedding_provider
            .get_or_init(|| async {
                let provider: Arc<dyn EmbeddingProvider + Send + Sync> =
                    match tokio::task::spawn_blocking(|| {
                        FastEmbedProvider::try_new(FastEmbedModel::Default)
                    })
                    .await
                    {
                        Ok(Ok(p)) => {
                            tracing::info!(target: "soul.embed", "search-path FastEmbed ready");
                            Arc::new(p)
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(
                                target: "soul.embed",
                                error = %e,
                                "FastEmbed init failed — falling back to MockEmbeddingProvider"
                            );
                            Arc::new(MockEmbeddingProvider::nomic())
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "soul.embed",
                                error = %e,
                                "FastEmbed join failed — falling back to MockEmbeddingProvider"
                            );
                            Arc::new(MockEmbeddingProvider::nomic())
                        }
                    };
                provider
            })
            .await
            .clone()
    }

    /// Constructs a state for integration tests without spawning background tasks.
    ///
    /// Keeps tests hermetic — no AYIN connection attempts, no filesystem
    /// watcher, no external dependencies.  The broadcast channel is still
    /// wired so tests can publish synthetic events by calling
    /// `state.event_tx.send(...)` directly.
    ///
    /// # For testing only
    ///
    /// This constructor is intended exclusively for integration test harnesses.
    /// Use [`AppState::new`] in production code.
    #[must_use]
    pub fn for_test(config: Config) -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_BUF);
        let active_agent = Arc::new(RwLock::new(config.agent.clone()));
        Self {
            config: Arc::new(config),
            turnlog_pepper: Arc::new(secrecy::SecretSlice::from(vec![])),
            session_count: Arc::new(AtomicUsize::new(0)),
            event_tx,
            browser_state: Arc::new(RwLock::new(BrowserStateSnapshot::default())),
            builds_cache: builds_handler::build_cache(),
            builds: Arc::new(BuildRegistry::new()),
            active_agent,
            soul_store: None,
            promotion_policy: None,
            _policy_watcher: None,
            embedding_provider: Arc::new(tokio::sync::OnceCell::new()),
        }
    }
}

/// Builds the Axum router with all routes wired.
///
/// - `GET /api/health` — liveness probe (unauthenticated).
/// - `GET /api/auth-check` — validates `Authorization: Bearer <token>`.
/// - `GET /api/terminal/ws` — PTY WebSocket bridge (Phase 2).
/// - `GET /api/events` — SSE fan-out stream (Phase 5, authenticated).
/// - `POST /api/control` — control command endpoint (authenticated).
/// - `GET /api/builds` — active build tracking (authenticated).
/// - `GET /api/browser-state` — read current browser UI state.
/// - `POST /api/browser-state` — update browser UI state (from frontend).
/// - `GET /api/polytopes` — per-sibling 4D polytope assignments (authenticated).
/// - Fallback — serves the embedded `../lightarchitects-webshell-ui/dist/` bundle.
///
/// `Router` is already `#[must_use]` so this function is not re-annotated.
#[allow(clippy::too_many_lines)]
pub fn build_app(state: AppState) -> Router {
    let cors = build_cors(state.config.port);
    Router::new()
        .route("/api/health", get(health))
        .route("/api/auth-check", get(auth_check))
        .route("/api/terminal/ws", get(terminal::ws::ws_handler))
        .route("/api/events", get(events::sse_handler::sse_handler))
        .route("/api/control", post(events::control_handler))
        .route(
            "/api/builds",
            get(builds_handler::builds_handler).post(builds_handler::create_build_handler),
        )
        .route("/api/lasdlc", get(builds_handler::lasdlc_meta_handler))
        .route(
            "/api/builds/plan",
            post(builds_handler::create_plan_handler),
        )
        .route(
            "/api/builds/plan/{codename}",
            put(builds_handler::update_plan_handler),
        )
        .route(
            "/api/builds/{id}",
            get(builds_handler::build_details_handler),
        )
        .route(
            "/api/builds/{id}/events",
            get(events::sse_handler::sse_build_handler),
        )
        .route(
            "/api/builds/{id}/notify",
            post(events::notify::notify_handler),
        )
        .route(
            "/api/builds/{id}/terminal/ws",
            get(terminal::ws::ws_build_handler),
        )
        .route(
            "/api/browser-state",
            get(read_browser_state).post(write_browser_state),
        )
        .route("/api/polytopes", get(polytopes))
        // ── Phase 9.5 / 10.5 SOUL vault hybrid memory routes ─────────────────
        .route("/api/soul/search", get(events::soul_routes::search_handler))
        .route(
            "/api/soul/entries/{*path}",
            get(events::soul_routes::entry_handler),
        )
        .route(
            "/api/soul/memory/hot",
            get(events::soul_routes::hot_memory_handler),
        )
        .route(
            "/api/soul/memory/cold",
            get(events::soul_routes::cold_memory_handler),
        )
        .route(
            "/api/soul/health",
            get(events::soul_routes::health_handler),
        )
        .route(
            "/api/soul/reindex",
            post(events::soul_routes::reindex_handler),
        )
        // Phase 16a — compaction preview (dry-run).
        .route(
            "/api/soul/compaction/preview",
            post(events::soul_routes::compaction_preview_handler),
        )
        // Phase 16b — destructive apply. Re-classifies at apply time,
        // then moves candidate files to .compacted/{date}/. Reversible
        // via manual mv (no scheduled prune).
        .route(
            "/api/soul/compaction/apply",
            post(events::soul_routes::compaction_apply_handler),
        )
        .route(
            "/api/soul/relationships/{*entry_id}",
            get(events::soul_routes::relationships_handler),
        )
        .route(
            "/api/soul/edges",
            get(events::soul_routes::edges_handler),
        )
        .route(
            "/api/soul/convergences",
            get(events::soul_routes::convergences_handler),
        )
        // ── Phase 20b.3: parity verification endpoint ─────────────────────
        .route(
            "/api/debug/parity",
            get(events::soul_routes::parity_handler),
        )
        // ── Phase 9.8–9.10: real-data handlers (replaces mock_data::*) ──────
        .route("/api/workspaces", get(real_data::list_workspaces))
        .route("/api/workspaces/{id}", get(real_data::get_workspace))
        .route("/api/meta-skills", get(real_data::list_meta_skills))
        .route("/api/siblings", get(real_data::get_sibling_status))
        .route("/api/sitrep", get(real_data::get_sitrep))
        .route("/api/conductor/status", get(real_data::get_conductor_status))
        .route("/api/arena/status", get(real_data::get_arena_status))
        .route(
            "/api/builds/{id}/findings",
            get(real_data::list_findings),
        )
        .route(
            "/api/builds/{id}/notes",
            get(real_data::get_notes).put(real_data::update_notes),
        )
        .route(
            "/api/builds/{id}/artifacts",
            get(real_data::list_artifacts).post(real_data::upload_artifact),
        )
        .route(
            "/api/builds/{id}/gates/{pillar}",
            get(real_data::get_gate_status),
        )
        .route(
            "/api/builds/{id}/pillars/{pillar}",
            post(real_data::trigger_pillar),
        )
        .route(
            "/api/builds/{id}/copilot",
            post(copilot::copilot_chat_handler),
        )
        .route(
            "/api/builds/{id}/dispatch",
            post(real_data::dispatch_sibling),
        )
        // ── Session fork (webshell → terminal handoff) ───────────────────────
        .route("/api/session/fork", post(session_fork::fork_handler))
        // ── Setup / backend-switch routes ────────────────────────────────────
        .route("/api/setup/info", get(setup::setup_info))
        .route("/api/setup/models", get(setup::setup_models))
        .route("/api/setup/save", post(setup::setup_save))
        .route("/api/setup/reset", axum::routing::delete(setup::setup_reset))
        .fallback(static_assets::serve)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Constructs a CORS layer that restricts allowed origins to localhost only.
///
/// This is a local-dev-only tool — binding to 127.0.0.1 narrows the attack
/// surface, but an explicit origin allowlist prevents arbitrary origins from
/// reading the authenticated SSE stream via a malicious browser tab.
///
/// Allowed origins:
/// - `http://localhost:<port>` — production (same-origin serving)
/// - `http://127.0.0.1:<port>` — same binary, loopback alias
/// - `http://localhost:5173` — Vite dev server during frontend development
fn build_cors(port: u16) -> CorsLayer {
    let allowed_origins: Vec<HeaderValue> = [
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
        "http://localhost:5173".to_owned(),
    ]
    .iter()
    .filter_map(|s| s.parse().ok())
    .collect();

    // `x-la-notify-token` is the per-build shared-secret header the gateway
    // uses to POST events to `/api/builds/:id/notify`. The gateway runs
    // server-side (reqwest ignores CORS), so allowing it globally is purely
    // to unblock browser-side testing/debug; the production browser never
    // holds this token and so never sends it.
    let notify_header: HeaderName = HeaderName::from_static("x-la-notify-token");
    CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            notify_header,
        ])
}

/// Errors surfaced by the server's main run loop.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Failed to bind the TCP listener to the configured port.
    #[error("failed to bind webshell server on 127.0.0.1:{port}: {source}")]
    Bind {
        /// Port that failed to bind.
        port: u16,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },

    /// Server exited with an IO error mid-run.
    #[error("webshell server exited with error: {0}")]
    Serve(#[source] std::io::Error),
}

/// Starts the webshell server on `127.0.0.1:<port>` and blocks until it exits.
///
/// # Errors
///
/// - [`ServerError::Bind`] if the TCP listener cannot bind to the configured port.
/// - [`ServerError::Serve`] if the server exits with an IO error mid-run.
pub async fn run(config: Config) -> Result<(), ServerError> {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    let state = AppState::new(config);
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|source| ServerError::Bind {
            port: addr.port(),
            source,
        })?;

    info!(bind = %addr, "webshell server listening");

    axum::serve(listener, app).await.map_err(ServerError::Serve)
}

/// `GET /api/health` — unauthenticated liveness probe.
async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// `GET /api/auth-check` — validates `Authorization: Bearer <token>`.
///
/// Responds 200 on match, 401 on mismatch or missing header.
async fn auth_check(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(authz) = headers.get("authorization") else {
        return StatusCode::UNAUTHORIZED;
    };

    let Ok(authz_str) = authz.to_str() else {
        return StatusCode::UNAUTHORIZED;
    };

    if auth::validate_bearer(authz_str, &state.config.token) {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

/// `GET /api/browser-state` — returns the latest browser UI state snapshot.
///
/// Authenticated — requires a valid `Authorization: Bearer <token>` header.
async fn read_browser_state(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(authz) = headers.get("authorization") else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(authz_str) = authz.to_str() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !auth::validate_bearer(authz_str, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let snapshot = state.browser_state.read().await;
    Json(snapshot.clone()).into_response()
}

/// `POST /api/browser-state` — updates the browser UI state snapshot.
///
/// Called periodically by the frontend to report current viewport, panel
/// sizes, zoom level, etc. Authenticated — requires a valid bearer token.
async fn write_browser_state(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(update): Json<BrowserStateSnapshot>,
) -> impl IntoResponse {
    let Some(authz) = headers.get("authorization") else {
        return StatusCode::UNAUTHORIZED;
    };
    let Ok(authz_str) = authz.to_str() else {
        return StatusCode::UNAUTHORIZED;
    };
    if !auth::validate_bearer(authz_str, &state.config.token) {
        return StatusCode::UNAUTHORIZED;
    }

    let mut snapshot = state.browser_state.write().await;
    *snapshot = update;

    StatusCode::OK
}

/// `GET /api/polytopes` — returns the per-sibling 4D polytope snapshot.
///
/// The payload is the compile-time-embedded [`polytope_data::POLYTOPES_JSON`]
/// (snapshot from `lightarchitects-next/src/app/data/projects.ts`). Authenticated —
/// requires `Authorization: Bearer <token>`.
///
/// Returns:
/// - `200 OK` with `application/json` body on valid token.
/// - `401 UNAUTHORIZED` on missing or invalid token.
async fn polytopes(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    let Some(authz) = headers.get("authorization") else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(authz_str) = authz.to_str() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !auth::validate_bearer(authz_str, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        polytope_data::POLYTOPES_JSON,
    )
        .into_response()
}

/// Loads the turnlog pepper from the canonical session key path.
///
/// Returns an empty secret if the key file is missing or unreadable —
/// turnlog will be disabled gracefully for the session.
fn load_turnlog_pepper() -> secrecy::SecretSlice<u8> {
    let Some(path) = lightarchitects::core::paths::session_key() else {
        tracing::warn!(target: "turnlog", "Session key path unavailable — turnlog disabled");
        return secrecy::SecretSlice::from(vec![]);
    };
    match std::fs::read(&path) {
        Ok(bytes) if !bytes.is_empty() => {
            tracing::info!(target: "turnlog", "Turnlog pepper loaded ({} bytes)", bytes.len());
            secrecy::SecretSlice::from(bytes)
        }
        Ok(_) => {
            tracing::warn!(target: "turnlog", "Session key empty — turnlog disabled");
            secrecy::SecretSlice::from(vec![])
        }
        Err(e) => {
            tracing::warn!(target: "turnlog", "Failed to read session key: {e} — turnlog disabled");
            secrecy::SecretSlice::from(vec![])
        }
    }
}
