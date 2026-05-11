//! Axum server: app construction, shared state, routes, run loop.
//!
//! Phase 1 wires three concerns: a liveness probe, an auth-check endpoint
//! that exercises the HMAC comparator, and a rust-embed static-asset
//! fallback serving the frontend bundle.
//!
//! Phase 2 adds `/api/terminal/ws` (PTY WebSocket bridge).
//! Phase 3/5 will add `/api/events` (SSE fan-out).

use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, atomic::AtomicUsize},
    task::{Context, Poll},
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use tokio::sync::{Mutex, RwLock, broadcast};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use crate::{
    agent, auth,
    config::{AgentSession, Config},
    container::{DockerCapability, ImageManager},
    coordination, copilot, csp,
    dispatch::{self, DispatchRegistry},
    events::{self, EVENT_CHANNEL_BUF, WebEvent, builds_handler},
    init::telemetry::TelemetryHandle,
    polytope_data, real_data,
    session::BuildRegistry,
    session_fork,
    session_store::SessionStore,
    setup, static_assets, terminal,
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

    /// Active Squad Dispatch registry.
    ///
    /// Stores in-flight dispatch handles keyed by [`dispatch::DispatchId`].
    /// Guarded by a `Mutex` — each registry operation is a short critical
    /// section with no long-held locks (MED M-4).
    pub dispatch_registry: Arc<Mutex<DispatchRegistry>>,
    /// Docker capability detected at startup.
    pub docker_capable: DockerCapability,
    /// Lazy image provisioning for containerized sessions.
    pub image_manager: ImageManager,
    /// 1P telemetry event sink (structured tracing, no PII).
    pub telemetry: TelemetryHandle,
    /// `SQLite` session persistence — survives browser refreshes and restarts.
    pub session_store: Arc<std::sync::Mutex<SessionStore>>,
}

impl AppState {
    /// Constructs a new state from a resolved [`Config`] and spawns the
    /// background AYIN SSE subscription task.
    #[must_use]
    pub fn new(config: Config, docker_capable: DockerCapability) -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_BUF);
        events::AyinClient::spawn(event_tx.clone());
        events::HelixWatcher::spawn(event_tx.clone());
        let pepper = load_turnlog_pepper();
        let active_agent = Arc::new(RwLock::new(config.agent.clone()));
        let soul_store = Some(Arc::new(crate::memory::persistence::SoulPersistence::open()));
        let telemetry = TelemetryHandle::new();
        let session_store = Arc::new(std::sync::Mutex::new(
            SessionStore::open().unwrap_or_else(|_| SessionStore::noop()),
        ));
        let image_manager = ImageManager::new(docker_capable);

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
            dispatch_registry: Arc::new(Mutex::new(DispatchRegistry::new())),
            docker_capable,
            image_manager,
            telemetry,
            session_store,
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
    pub fn for_test(config: Config, docker_capable: DockerCapability) -> Self {
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
            dispatch_registry: Arc::new(Mutex::new(DispatchRegistry::new())),
            docker_capable,
            image_manager: ImageManager::new(docker_capable),
            telemetry: TelemetryHandle::new(),
            session_store: Arc::new(std::sync::Mutex::new(SessionStore::noop())),
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
        .route("/api/auth/exchange", post(auth_exchange))
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/session", delete(auth_logout))
        .route("/api/terminal/ws", get(terminal::ws::ws_handler))
        .route("/api/events", get(events::sse_handler::sse_handler))
        .route("/api/control", post(events::control_handler))
        .route(
            "/api/builds",
            get(builds_handler::builds_handler).post(builds_handler::create_build_handler),
        )
        .route("/api/builds/resume", get(resume_sessions_handler))
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
        // ── Option E: hybrid SSE + WebSocket agent protocol ────────────────
        .route(
            "/api/builds/{id}/agent/stream",
            get(agent::sse::agent_sse_handler),
        )
        .route(
            "/api/builds/{id}/agent/ws",
            get(agent::ws::agent_ws_handler),
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
        .route("/api/siblings", get(real_data::get_squad_status))
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
        // ── Squad Comms (coordination) routes ───────────────────────────────
        .route(
            "/api/coordination/tasks",
            get(coordination::list_tasks),
        )
        .route(
            "/api/coordination/tasks/add",
            post(coordination::add_task),
        )
        .route(
            "/api/coordination/tasks/claim/{id}",
            post(coordination::claim_task),
        )
        .route(
            "/api/coordination/tasks/{id}/logs",
            get(coordination::task_logs),
        )
        .route(
            "/api/coordination/chat/sessions",
            get(coordination::chat_sessions),
        )
        .route(
            "/api/coordination/chat/inject",
            post(coordination::chat_inject),
        )
        .route(
            "/api/coordination/chat/stream",
            get(coordination::chat_stream),
        )
        // ── Squad Dispatch routes — all Bearer-authenticated (HIGH H-5) ─────
        .merge(dispatch::dispatch_router())
        // ── File listing for @-file autocomplete ─────────────────────────────
        .route("/api/files", get(list_files_handler))
        // ── CSP violation reports (SEC-3b, Enforce phase) ────────────────────
        .route("/api/csp-report", post(csp::csp_report_handler))
        .fallback(static_assets::serve)
        // SEC-3b: Enforce-mode CSP — violations are blocked.
        .layer(axum::middleware::from_fn(csp::enforce_layer))
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
/// The Vite dev-server origin (`localhost:5173`) is included in debug builds
/// only; release binaries serve the pre-built bundle from the same port and
/// must not accept cross-origin requests from other local ports.
///
/// Allowed origins:
/// - `http://localhost:<port>` — production (same-origin serving)
/// - `http://127.0.0.1:<port>` — same binary, loopback alias
/// - `http://localhost:5173` — Vite dev server (**debug builds only**)
fn build_cors(port: u16) -> CorsLayer {
    let mut origins: Vec<String> = vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
    ];
    // Vite dev server origin: present only in debug builds so that release
    // binaries cannot be cross-origin requested from an arbitrary port.
    #[cfg(debug_assertions)]
    origins.push("http://localhost:5173".to_owned());

    let allowed_origins: Vec<HeaderValue> = origins.iter().filter_map(|s| s.parse().ok()).collect();

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

    /// Every port in the auto-retry range was busy.
    ///
    /// Carries the first port that was tried (the one the user configured)
    /// so the caller can emit a diagnostic pointing at the right address.
    #[error("port 127.0.0.1:{first_port} (and {tried} fallback(s)) are all in use")]
    PortInUse {
        /// The port originally requested via config / `--port`.
        first_port: u16,
        /// Number of fallback ports that were also tried.
        tried: u8,
    },

    /// Server exited with an IO error mid-run.
    #[error("webshell server exited with error: {0}")]
    Serve(#[source] std::io::Error),
}

/// Opaque future that drives the axum server until it exits.
///
/// Created by [`run_with_port_retry`] after a successful bind.  The caller
/// should `await` this future (often inside `tokio::select!`) to keep the
/// server alive.
pub struct ServerDriver {
    inner: Pin<Box<dyn Future<Output = Result<(), ServerError>> + Send>>,
}

impl Future for ServerDriver {
    type Output = Result<(), ServerError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

/// Starts the webshell server on `127.0.0.1:<port>` and blocks until it exits.
///
/// # Errors
///
/// - [`ServerError::Bind`] if the TCP listener cannot bind to the configured port.
/// - [`ServerError::Serve`] if the server exits with an IO error mid-run.
pub async fn run(config: Config, docker_capable: DockerCapability) -> Result<(), ServerError> {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    let state = AppState::new(config, docker_capable);
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

/// Maximum number of sequential ports tried beyond the configured port.
const PORT_RETRY_LIMIT: u8 = 3;

/// Binds the webshell server, automatically retrying on adjacent ports when
/// the configured port is already in use (`EADDRINUSE`).
///
/// On success, returns the port that was actually bound **and** a [`ServerDriver`]
/// future that must be awaited to keep the server running.  This split lets the
/// caller print the real port (e.g. fallback) and fire profiler checkpoints
/// *before* the server blocks.
///
/// On failure, returns [`ServerError::PortInUse`] when all retried ports were
/// busy, or [`ServerError::Bind`] for other bind failures.
///
/// # Errors
///
/// - [`ServerError::PortInUse`] when every port in the retry window is busy.
/// - [`ServerError::Bind`] when the port is available but binding fails for
///   another reason (e.g., permission denied on port < 1024).
pub async fn run_with_port_retry(
    config: Config,
    docker_capable: DockerCapability,
) -> Result<(u16, ServerDriver), ServerError> {
    use std::io::ErrorKind;

    let first_port = config.port;

    for attempt in 0..=PORT_RETRY_LIMIT {
        // SAFE: `attempt` ≤ PORT_RETRY_LIMIT ≤ 3; first_port is u16. In the
        // pathological case where first_port is near u16::MAX, wrapping_add
        // avoids a panic; the resulting port will fail to bind anyway.
        let port = first_port.wrapping_add(u16::from(attempt));
        let mut try_config = config.clone();
        try_config.port = port;

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                let state = AppState::new(try_config, docker_capable);
                let app = build_app(state);
                info!(bind = %addr, "webshell server listening");
                let driver = ServerDriver {
                    inner: Box::pin(async move {
                        axum::serve(listener, app)
                            .with_graceful_shutdown(crate::init::shutdown::shutdown_signal())
                            .await
                            .map_err(ServerError::Serve)
                    }),
                };
                return Ok((port, driver));
            }
            Err(e) if e.kind() == ErrorKind::AddrInUse => {
                if attempt < PORT_RETRY_LIMIT {
                    info!(
                        port,
                        next = port.wrapping_add(1),
                        "port in use, trying next port"
                    );
                } else {
                    return Err(ServerError::PortInUse {
                        first_port,
                        tried: PORT_RETRY_LIMIT,
                    });
                }
            }
            Err(source) => {
                return Err(ServerError::Bind { port, source });
            }
        }
    }

    // Unreachable: the loop above always returns in the final iteration.
    unreachable!("port retry loop exhausted without returning")
}

// ── File listing — @-file autocomplete support ───────────────────────────────

use axum::extract::Query as AxumQuery;

/// `GET /api/files?q=<query>` — returns relative paths under `cwd` matching
/// the query string.
///
/// Results are capped at 50, walk depth at 5. Hidden dirs and common
/// build-artifact dirs (`target`, `node_modules`, `.git`, etc.) are skipped.
/// Requires `Authorization: Bearer <token>`.
async fn list_files_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumQuery(params): AxumQuery<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(authz) = headers.get("authorization") else {
        return (StatusCode::UNAUTHORIZED, Json(Vec::<String>::new()));
    };
    let Ok(authz_str) = authz.to_str() else {
        return (StatusCode::UNAUTHORIZED, Json(Vec::<String>::new()));
    };
    if !auth::validate_bearer(authz_str, &state.config.token) {
        return (StatusCode::UNAUTHORIZED, Json(Vec::<String>::new()));
    }

    let query = params
        .get("q")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    let cwd = state.config.cwd.clone();
    let matches = tokio::task::spawn_blocking(move || walk_files(&cwd, &query))
        .await
        .unwrap_or_default();
    (StatusCode::OK, Json(matches))
}

/// BFS walk of `root` collecting relative paths whose filename contains `query`.
///
/// Caps at 50 results and depth 5. Skips hidden entries and common
/// build-artifact directories.
fn walk_files(root: &std::path::Path, query: &str) -> Vec<String> {
    const MAX_DEPTH: usize = 5;
    const MAX_RESULTS: usize = 50;
    const SKIP_DIRS: &[&str] = &[
        "target",
        "node_modules",
        ".git",
        ".svelte-kit",
        "dist",
        "__pycache__",
        ".cargo",
        "build",
        "out",
    ];

    let mut queue: std::collections::VecDeque<(std::path::PathBuf, usize)> =
        std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));
    let mut results: Vec<String> = Vec::new();

    while let Some((dir, depth)) = queue.pop_front() {
        if depth > MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let raw_name = entry.file_name();
            let name = raw_name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                queue.push_back((path, depth + 1));
            } else if path.is_file() && (query.is_empty() || name.to_lowercase().contains(query)) {
                if let Ok(rel) = path.strip_prefix(root) {
                    results.push(rel.to_string_lossy().into_owned());
                    if results.len() >= MAX_RESULTS {
                        return results;
                    }
                }
            }
        }
    }
    results
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

/// Request body for `POST /api/auth/exchange`.
///
/// Carries the bare session token for exchange against an `HttpOnly` cookie.
#[derive(serde::Deserialize)]
struct TokenExchange {
    /// The raw bearer token (no `Bearer ` prefix) to exchange for a cookie.
    token: String,
}

/// `POST /api/auth/exchange` — swaps a Bearer token for an `HttpOnly` session cookie.
///
/// Unauthenticated endpoint — accepts a JSON body `{ "token": "<bare-token>" }`,
/// validates it in constant time, then responds 200 with `Set-Cookie: la_session=...`.
/// After exchange the frontend drops the token from sessionStorage and sends no
/// `Authorization` header — cookies flow automatically on same-origin requests.
async fn auth_exchange(
    State(state): State<AppState>,
    Json(body): Json<TokenExchange>,
) -> impl IntoResponse {
    if !auth::validate_raw_token(&body.token, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let cookie = auth::session_cookie_header(&state.config.token);
    match HeaderValue::from_str(&cookie) {
        Ok(cookie_val) => {
            tracing::info!(target: "webshell", "Cookie session established via exchange");
            (StatusCode::OK, [(header::SET_COOKIE, cookie_val)]).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// `GET /api/auth/status` — validates the `HttpOnly` session cookie and refreshes its TTL.
///
/// Returns 200 with a refreshed `Set-Cookie` on success, 401 on missing or invalid cookie.
/// Called every 30 minutes by the frontend to implement a sliding TTL.
async fn auth_status(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(cookie_hdr) = headers.get(header::COOKIE) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(cookie_str) = cookie_hdr.to_str() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !auth::validate_session_cookie(cookie_str, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let cookie = auth::session_cookie_header(&state.config.token);
    match HeaderValue::from_str(&cookie) {
        Ok(cookie_val) => {
            tracing::debug!(target: "webshell", "Cookie session TTL refreshed");
            (StatusCode::OK, [(header::SET_COOKIE, cookie_val)]).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// `DELETE /api/auth/session` — removes the persisted bearer token from file + keyring.
///
/// Accepts either `Authorization: Bearer <token>` or a valid `la_session` cookie.
/// Returns 204 No Content on success with an expired `Set-Cookie` to clear the cookie.
/// The server continues running with the in-memory token; the *next* startup generates
/// a fresh one.
async fn auth_logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let authorized = if let Some(authz) = headers.get("authorization") {
        authz
            .to_str()
            .is_ok_and(|s| auth::validate_bearer(s, &state.config.token))
    } else if let Some(cookie_hdr) = headers.get(header::COOKIE) {
        cookie_hdr
            .to_str()
            .is_ok_and(|s| auth::validate_session_cookie(s, &state.config.token))
    } else {
        false
    };
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    crate::config::remove_persisted_token();
    tracing::info!(target: "webshell", "Auth session logout: persisted token cleared");
    let cleared = HeaderValue::from_static(auth::clear_session_cookie_header());
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cleared)]).into_response()
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

/// `GET /api/builds/resume` — returns all persisted sessions from `SessionStore`.
///
/// Auth-gated (global Bearer token). Results are ordered by `updated_at`
/// descending (most recently touched first).
async fn resume_sessions_handler(
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

    let rows = match state.session_store.lock() {
        Ok(store) => store.list(),
        Err(e) => {
            tracing::error!(error = %e, "failed to lock session store");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match rows {
        Ok(sessions) => (StatusCode::OK, Json(sessions)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "session_store list failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
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

// ── walk_files unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod walk_tests {
    use super::walk_files;
    use std::fs;

    /// Create a temp directory tree and return its path.
    /// Caller must clean up with `fs::remove_dir_all`.
    fn make_tree(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("la_walk_test_{name}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn empty_query_returns_all_files() {
        let root = make_tree("empty_q");
        fs::write(root.join("a.rs"), "").unwrap();
        fs::write(root.join("b.ts"), "").unwrap();
        let results = walk_files(&root, "");
        assert!(results.contains(&"a.rs".to_owned()), "{results:?}");
        assert!(results.contains(&"b.ts".to_owned()), "{results:?}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn query_filters_by_filename_substring() {
        let root = make_tree("filter_q");
        fs::write(root.join("main.rs"), "").unwrap();
        fs::write(root.join("lib.rs"), "").unwrap();
        fs::write(root.join("config.toml"), "").unwrap();
        let results = walk_files(&root, "rs");
        assert!(
            results.iter().all(|p| std::path::Path::new(p)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("rs"))),
            "{results:?}"
        );
        assert_eq!(results.len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn query_is_case_insensitive() {
        let root = make_tree("case_q");
        fs::write(root.join("README.md"), "").unwrap();
        fs::write(root.join("other.txt"), "").unwrap();
        let results = walk_files(&root, "readme");
        assert_eq!(results.len(), 1);
        assert!(results[0].to_lowercase().contains("readme"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn results_are_relative_paths() {
        let root = make_tree("rel_paths");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("main.rs"), "").unwrap();
        let results = walk_files(&root, "main");
        assert_eq!(results, vec!["src/main.rs".to_owned()]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn skip_dirs_are_excluded() {
        let root = make_tree("skip_dirs");
        // Files in skip dirs should not appear
        for skip in &["target", "node_modules", ".git"] {
            fs::create_dir_all(root.join(skip)).unwrap();
            fs::write(root.join(skip).join("skip_me.rs"), "").unwrap();
        }
        fs::write(root.join("keep_me.rs"), "").unwrap();
        let results = walk_files(&root, "");
        assert!(results.contains(&"keep_me.rs".to_owned()), "{results:?}");
        assert!(
            !results.iter().any(|p| p.contains("skip_me")),
            "{results:?}"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hidden_files_are_excluded() {
        let root = make_tree("hidden");
        fs::write(root.join(".hidden_file"), "").unwrap();
        fs::write(root.join("visible.rs"), "").unwrap();
        let results = walk_files(&root, "");
        assert!(!results.iter().any(|p| p.starts_with('.')), "{results:?}");
        assert!(results.contains(&"visible.rs".to_owned()), "{results:?}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn max_results_cap_is_honoured() {
        let root = make_tree("max_results");
        for i in 0..60_u8 {
            fs::write(root.join(format!("file_{i:03}.rs")), "").unwrap();
        }
        let results = walk_files(&root, "");
        assert!(results.len() <= 50, "got {} results", results.len());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn deep_tree_respects_depth_limit() {
        let root = make_tree("depth_limit");
        // Create a chain 8 dirs deep with a file at the bottom
        let mut p = root.clone();
        for i in 0..8_u8 {
            p = p.join(format!("level_{i}"));
            fs::create_dir_all(&p).unwrap();
        }
        fs::write(p.join("deep.rs"), "").unwrap();
        // Shallow file should be found; deep file should not (depth > 5)
        fs::write(root.join("shallow.rs"), "").unwrap();
        let results = walk_files(&root, "");
        assert!(results.contains(&"shallow.rs".to_owned()), "{results:?}");
        assert!(
            !results.iter().any(|p| p.contains("deep.rs")),
            "{results:?}"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_results_when_no_match() {
        let root = make_tree("no_match");
        fs::write(root.join("foo.rs"), "").unwrap();
        let results = walk_files(&root, "zzznomatch");
        assert!(results.is_empty(), "{results:?}");
        fs::remove_dir_all(&root).unwrap();
    }
}
