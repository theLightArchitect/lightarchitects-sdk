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
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use dashmap::DashMap;
use uuid::Uuid;

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
    events::{self, EVENT_CHANNEL_BUF, WebEventV2, builds_handler},
    gitforest,
    init::telemetry::TelemetryHandle,
    polytope_data, preflight,
    preflight::{OverallStatus, PreflightReport},
    real_data,
    session::BuildRegistry,
    session_fork,
    session_store::SessionStore,
    setup, static_assets, terminal,
};

pub mod code_routes;
pub mod exec_routes;
pub mod fleet_routes;
pub mod git_routes;
pub mod mcp_routes;
pub mod roadmap;

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
    pub event_tx: broadcast::Sender<WebEventV2>,
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
    /// One-time auth nonces registered by the gateway at webshell launch.
    ///
    /// Maps [`Uuid`] nonce → expiry [`std::time::Instant`] (60-second TTL).
    /// Consumed on first use; expired entries are discarded on access.
    pub auth_nonces: Arc<DashMap<Uuid, std::time::Instant>>,
    /// OAuth CSRF state map — keyed by state UUID (OA-2).
    ///
    /// Separate from `auth_nonces`: different TTL (120 s vs 60 s) and a
    /// provider-specific payload (`OAuthPendingState`) that `auth_nonces`
    /// does not carry.  Entries are consumed on first valid callback or
    /// evicted at TTL.
    pub oauth_states: Arc<DashMap<Uuid, crate::auth::credential::OAuthPendingState>>,
    /// Provider connection state cache — keyed by provider identifier.
    ///
    /// Avoids a Keychain subprocess call on every status request.
    /// Written by init / callback / revoke handlers; read on status checks.
    pub credential_store: Arc<DashMap<String, crate::auth::credential::CredentialState>>,
    /// Global event ring buffer — plan-builder-copilot-bridge Phase 3.
    ///
    /// Stores the last 1,000 [`GlobalEventEntry`] entries across all sources
    /// (copilot subprocesses, conductor workers, gate runners). Subscribers
    /// connect via `GET /api/events/global` (SSE). Filtering is applied
    /// consumer-side per [`EventFilter`] query params.
    pub global_event_store: events::GlobalEventStore,
    /// In-flight plan draft sessions — keyed by pre-minted session `UUID`.
    ///
    /// Broadcast sender per in-flight plan draft session.
    ///
    /// `broadcast::Sender` allows multiple `SSE` subscribers (browser tab refresh
    /// safety). Each entry is inserted on `POST /api/builds/plan/draft` and
    /// removed when `Done`/`Error` fires or the session TTL expires.
    /// The paired [`tokio_util::sync::CancellationToken`] lets the `SSE` handler
    /// signal subprocess cancellation on client disconnect.
    pub plan_draft_sessions: Arc<
        DashMap<
            uuid::Uuid,
            (
                tokio::sync::broadcast::Sender<crate::events::types::PlanDraftEvent>,
                tokio_util::sync::CancellationToken,
            ),
        >,
    >,
    /// Per-build northstar supervisor state — keyed by build [`Uuid`].
    ///
    /// Inserted by `POST /api/builds` when `northstar_text` is present.
    /// The background watcher task holds an `Arc` clone and exits when
    /// `SupervisorEntry::watcher_token` is cancelled.
    pub supervisor_states: Arc<DashMap<Uuid, Arc<events::SupervisorEntry>>>,
    /// In-flight autonomous build handles — keyed by build [`Uuid`].
    ///
    /// Inserted by `POST /api/builds` when `mode = "autonomous"`.
    /// The `JoinHandle` can be aborted on build cancellation; it resolves
    /// when all waves complete or the first `WaveError` halts the run.
    pub lightsquad_programs: Arc<DashMap<Uuid, tokio::task::JoinHandle<()>>>,
    /// Directory where per-build NDJSON decision logs are written.
    ///
    /// One file per build: `<decisions_dir>/<build_id>.ndjson`.
    /// Defaults to `~/.lightarchitects/builds/decisions/`; overridden to
    /// a temp dir in the test harness so tests don't pollute the vault.
    pub decisions_dir: std::path::PathBuf,
    /// When `true`, autonomous builds use the hermetic mock worker (write file +
    /// git commit) instead of spawning the real `lightarchitects --bare` CLI.
    ///
    /// Set by [`AppState::for_test`]; always `false` in production.
    pub mock_workers: bool,
    /// Structured infrastructure readiness report — populated at startup by
    /// [`preflight::run_full`] and updated by `POST /api/preflight/refresh`.
    ///
    /// Always 200 on `GET /api/preflight`; body carries status. Monitoring
    /// systems MUST NOT restart the process on `overall: Blocked` alone.
    /// See §2.5 for the `/api/health` vs `/api/preflight` semantic distinction.
    pub preflight: Arc<RwLock<PreflightReport>>,
    /// Unix epoch seconds of the last `POST /api/preflight/refresh` call.
    ///
    /// Used to enforce the 1-per-10s rate limit that prevents keychain ACL
    /// dialog spam when the macOS keychain is locked.
    preflight_last_refresh: Arc<AtomicU64>,
    /// `GitForest` topology cache — 60s TTL, max 64 repos (Phase 4 Agent A).
    pub gitforest_cache: crate::gitforest::routes::TopologyMokaCache,
    /// GitHub CI check-run cache — 60s TTL, max 512 SHAs (Phase 4 Agent B).
    pub check_run_cache: crate::github_proxy::CheckRunCache,
    /// HITL PR search cache — 60s TTL, keyed by `"me"` (webshell-hitl-inbox Phase 1).
    pub hitl_search_cache: crate::github_proxy::HitlSearchCache,
    /// PR metadata cache — 60s TTL, max 256 entries keyed by `"{owner}/{repo}/{number}"`.
    pub pr_metadata_cache: crate::github_proxy::PrMetadataCache,
    /// Commit metadata cache — 60s TTL, max 512 entries keyed by `"{owner}/{repo}/{sha}"`.
    pub commit_metadata_cache: crate::github_proxy::CommitMetadataCache,
    /// Path to the pre-generated roadmap HTML artifact served by `GET /api/roadmap`.
    ///
    /// Written by `/SYNC --roadmap`; defaults to
    /// `$HOME/lightarchitects/soul/helix/corso/builds/roadmap.html`.
    /// The handler returns an empty body when the file is absent (→ `empty` state).
    pub roadmap_html_path: std::path::PathBuf,
    /// EVA identity cache — frontmatter-stripped body of `eva/identity.md`.
    ///
    /// Background task is the sole writer (30s poll); per-request read lock only —
    /// no file I/O on the hot path.  Empty when the file is absent or unreadable.
    pub eva_identity: Arc<tokio::sync::RwLock<crate::copilot::eva_identity::EvaIdentityCache>>,
    /// Optional MCP host — spawned from `~/.lightarchitects/webshell-mcp.json`
    /// at startup. `None` until Phase 7 places the config file.
    pub mcp_host: mcp_routes::McpHostHandle,
}

impl AppState {
    /// Constructs a new state from a resolved [`Config`] and spawns the
    /// background AYIN SSE subscription task.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn new(
        config: Config,
        docker_capable: DockerCapability,
        preflight: PreflightReport,
    ) -> Self {
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
        let identity_path =
            lightarchitects::core::paths::helix_root_or_fallback().join("eva/identity.md");
        let eva_identity = Arc::new(tokio::sync::RwLock::new(
            crate::copilot::eva_identity::EvaIdentityCache::load(&identity_path),
        ));
        {
            let cache = eva_identity.clone();
            let path = identity_path.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    cache.write().await.check_reload(&path);
                }
            });
        }

        // F9: create oauth_states before Self so the eviction task can hold an Arc clone.
        let oauth_states: Arc<DashMap<Uuid, crate::auth::credential::OAuthPendingState>> =
            Arc::new(DashMap::new());
        {
            let eviction_states = Arc::clone(&oauth_states);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let now = std::time::Instant::now();
                    eviction_states.retain(|_, v| now < v.expires_at);
                }
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
            auth_nonces: Arc::new(DashMap::new()),
            oauth_states,
            credential_store: Arc::new(DashMap::new()),
            global_event_store: {
                let data_dir = std::env::var("HOME").map_or_else(
                    |_| std::path::PathBuf::from("/tmp").join("lightarchitects-webshell"),
                    |h| {
                        std::path::PathBuf::from(h)
                            .join(".lightarchitects")
                            .join("webshell")
                    },
                );
                let _ = std::fs::create_dir_all(&data_dir);
                events::GlobalEventStore::new(Some(data_dir.join("events.ndjson")))
            },
            plan_draft_sessions: Arc::new(DashMap::new()),
            supervisor_states: Arc::new(DashMap::new()),
            lightsquad_programs: Arc::new(DashMap::new()),
            decisions_dir: std::env::var("HOME").map_or_else(
                |_| std::path::PathBuf::from("/tmp").join("la-decisions"),
                |h| {
                    std::path::PathBuf::from(h)
                        .join(".lightarchitects")
                        .join("builds")
                        .join("decisions")
                },
            ),
            roadmap_html_path: std::env::var("HOME").map_or_else(
                |_| std::path::PathBuf::from("/tmp").join("roadmap.html"),
                |h| {
                    std::path::PathBuf::from(h)
                        .join("lightarchitects")
                        .join("soul")
                        .join("helix")
                        .join("corso")
                        .join("builds")
                        .join("roadmap.html")
                },
            ),
            mock_workers: false,
            preflight: Arc::new(RwLock::new(preflight)),
            preflight_last_refresh: Arc::new(AtomicU64::new(0)),
            gitforest_cache: crate::gitforest::routes::topology_cache(),
            check_run_cache: crate::github_proxy::check_run_cache(),
            hitl_search_cache: crate::github_proxy::hitl_search_cache(),
            pr_metadata_cache: crate::github_proxy::pr_metadata_cache(),
            commit_metadata_cache: crate::github_proxy::commit_metadata_cache(),
            eva_identity,
            mcp_host: {
                let handle = std::sync::Arc::new(tokio::sync::RwLock::new(None));
                let h2 = handle.clone();
                tokio::spawn(async move {
                    if let Some(mgr) = mcp_routes::try_init_host().await {
                        *h2.write().await = Some(mgr);
                    }
                });
                handle
            },
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
            auth_nonces: Arc::new(DashMap::new()),
            oauth_states: Arc::new(DashMap::new()),
            credential_store: Arc::new(DashMap::new()),
            global_event_store: events::GlobalEventStore::noop(),
            plan_draft_sessions: Arc::new(DashMap::new()),
            supervisor_states: Arc::new(DashMap::new()),
            lightsquad_programs: Arc::new(DashMap::new()),
            decisions_dir: std::env::temp_dir().join("la-decisions-test"),
            roadmap_html_path: std::env::temp_dir().join("la-roadmap-test.html"),
            mock_workers: true,
            preflight: Arc::new(RwLock::new(PreflightReport {
                timestamp: chrono::Utc::now(),
                overall: OverallStatus::Ready,
                checks: vec![],
                elapsed_ms: 0,
            })),
            preflight_last_refresh: Arc::new(AtomicU64::new(0)),
            gitforest_cache: crate::gitforest::routes::topology_cache(),
            check_run_cache: crate::github_proxy::check_run_cache(),
            hitl_search_cache: crate::github_proxy::hitl_search_cache(),
            pr_metadata_cache: crate::github_proxy::pr_metadata_cache(),
            commit_metadata_cache: crate::github_proxy::commit_metadata_cache(),
            eva_identity: Arc::new(tokio::sync::RwLock::new(
                crate::copilot::eva_identity::EvaIdentityCache::default(),
            )),
            mcp_host: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        }
    }
}

/// Builds the Axum router with all routes wired.
///
/// - `GET /api/health` — liveness probe (unauthenticated).
/// - `GET /api/preflight` — readiness probe, structured JSON (unauthenticated).
/// - `POST /api/preflight/refresh` — re-run all checks, rate-limited 1/10s (authenticated).
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
        .route("/api/preflight", get(preflight_status_handler))
        .route("/api/preflight/refresh", post(preflight_refresh_handler))
        .route("/api/auth-check", get(auth_check))
        .route("/api/auth/exchange", post(auth_exchange))
        .route("/api/auth/nonce", post(auth_issue_nonce))
        .route("/api/auth/nonce-exchange", post(auth_nonce_exchange))
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/session", delete(auth_logout))
        .route(
            "/api/auth/credential/google/init",
            post(crate::auth::credential::routes::google_init),
        )
        .route(
            "/api/auth/credential/google/callback",
            get(crate::auth::credential::routes::google_callback),
        )
        .route(
            "/api/auth/credential/github/device",
            post(crate::auth::credential::routes::github_device_init),
        )
        .route(
            "/api/auth/credential/github/poll",
            post(crate::auth::credential::routes::github_device_poll),
        )
        .route(
            "/api/auth/credential/ollama/connect",
            post(crate::auth::credential::routes::ollama_connect),
        )
        .route(
            "/api/auth/credential/{provider}/key",
            // DefaultBodyLimit rejects oversized bodies before Json deserialization (F10 — transport layer).
            // 2 KB covers the largest real API key formats with JSON framing; MAX_API_KEY_BYTES (1 KB)
            // is the secondary application-layer guard inside the handler.
            post(crate::auth::credential::routes::store_api_key)
                .layer(axum::extract::DefaultBodyLimit::max(2 * 1024)),
        )
        .route(
            "/api/auth/credential/{provider}/status",
            get(crate::auth::credential::routes::provider_status),
        )
        .route(
            "/api/auth/credential/{provider}",
            delete(crate::auth::credential::routes::provider_revoke),
        )
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
            "/api/builds/plan/draft",
            post(builds_handler::draft_plan_handler),
        )
        .route(
            "/api/builds/plan/draft-stream/{session_id}",
            get(builds_handler::plan_draft_stream_handler),
        )
        .route(
            "/api/builds/plan/commit",
            post(builds_handler::commit_plan_handler),
        )
        .route("/api/events/global", get(builds_handler::global_events_handler))
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
            "/api/builds/{id}/decisions",
            get(builds_handler::build_decisions_handler),
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
        // ── Fleet SSE + snapshot (agent-teams-fleet Phase 3) ────────────────
        .route(
            "/api/builds/{id}/fleet",
            get(fleet_routes::fleet_sse_handler),
        )
        .route(
            "/api/builds/{id}/fleet/snapshot",
            get(fleet_routes::fleet_snapshot_handler),
        )
        // ── Northstar supervisor (copilot-supervised-orchestration) ──────────
        .route(
            "/api/builds/{id}/supervisor/events",
            get(events::supervisor_handler::supervisor_sse_handler),
        )
        .route(
            "/api/builds/{id}/supervisor/state",
            get(events::supervisor_handler::supervisor_state_handler),
        )
        .route(
            "/api/builds/{id}/supervisor/acknowledge",
            post(events::supervisor_handler::supervisor_acknowledge_handler),
        )
        .route(
            "/api/browser-state",
            get(read_browser_state).post(write_browser_state),
        )
        .route("/api/polytopes", get(polytopes))
        // ── GitForest live operational map (Phase 4) ──────────────────────────
        .route(
            "/api/gitforest/topology",
            get(gitforest::routes::handle_topology),
        )
        .route(
            "/api/gitforest/live",
            get(gitforest::routes::handle_live),
        )
        .route(
            "/api/gitforest/node/{*id}",
            get(gitforest::routes::handle_node),
        )
        // ── Architecture Intelligence proxy (Phase 6 — M17) ───────────────────
        .route("/api/arch/extract", post(crate::arch_proxy::extract_handler))
        .route("/api/arch/verify",  post(crate::arch_proxy::verify_handler))
        .route("/api/arch/render",  post(crate::arch_proxy::render_handler))
        .route("/api/arch/emit",    post(crate::arch_proxy::emit_handler))
        .route("/api/arch/kroki",   post(crate::arch_proxy::kroki_handler))
        .route("/api/arch/health",  get(crate::arch_proxy::health_handler))
        // ── MCP host proxy (webshell-mcp-host Phase 5) ────────────────────────
        .route("/api/mcp/servers", get(mcp_routes::list_servers_handler))
        .route("/api/mcp/tools",   get(mcp_routes::list_tools_handler))
        .route("/api/mcp/invoke",  post(mcp_routes::invoke_handler))
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
        // ── helix-viz-remap: helix node snapshot (Helix3D cold-start) ──────
        .route(
            "/api/helix/nodes",
            get(events::soul_routes::helix_nodes_handler),
        )
        // ── Phase 20b.3: parity verification endpoint ─────────────────────
        .route(
            "/api/debug/parity",
            get(events::soul_routes::parity_handler),
        )
        // ── Project identity (webshell-project-ingestion §2.33-2.34) ────────
        .route("/api/projects", get(real_data::list_projects))
        .route(
            "/api/projects/init",
            post(crate::projects::init::init_project_handler),
        )
        .route("/api/projects/{slug}", get(real_data::get_project))
        // ── Phase 9.8–9.10: real-data handlers (replaces mock_data::*) ──────
        .route("/api/workspaces", get(real_data::list_workspaces))
        .route("/api/workspaces/{id}", get(real_data::get_workspace))
        .route("/api/meta-skills", get(real_data::list_meta_skills))
        .route("/api/siblings", get(real_data::get_squad_status))
        .route("/api/sitrep", get(real_data::get_sitrep))
        .route("/api/conductor/status", get(real_data::get_conductor_status))
        .route("/api/arena/status", get(real_data::get_arena_status))
        .route("/api/mcp-servers", get(real_data::list_mcp_servers))
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
            "/api/builds/{id}/copilot/voice",
            post(copilot::copilot_voice_handler),
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
            "/api/coordination/sessions/start",
            post(coordination::session_start),
        )
        .route(
            "/api/coordination/sessions/end",
            post(coordination::session_end),
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
        .route(
            "/api/coordination/tasks/spawn-worker",
            post(coordination::spawn_worker),
        )
        // ── Squad Dispatch routes — all Bearer-authenticated (HIGH H-5) ─────
        .merge(dispatch::dispatch_router())
        // ── File listing for @-file autocomplete ─────────────────────────────
        .route("/api/files", get(list_files_handler))
        // ── Process execution API (EEF Wave 2 — E2 gate) ─────────────────────
        .route("/api/exec/run", post(exec_routes::run_handler))
        .route("/api/exec/output/{handle}", get(exec_routes::output_handler))
        .route("/api/exec/processes", get(exec_routes::processes_handler))
        .route("/api/exec/kill", post(exec_routes::kill_handler))
        // ── Code editor API (EEF Phase 3 Wave 1) ─────────────────────────────
        .route("/api/code/read", get(code_routes::read_handler))
        .route("/api/code/list", get(code_routes::list_handler))
        .route("/api/code/write", post(code_routes::write_handler))
        .route("/api/code/search", post(code_routes::search_handler))
        .route(
            "/api/code/preview-diff",
            post(code_routes::preview_diff_handler),
        )
        .route(
            "/api/code/apply-diff",
            post(code_routes::apply_diff_handler),
        )
        // ── Git operations API (EEF Wave E3 — git-and-pr) ────────────────────
        .route("/api/git/status", post(git_routes::status_handler))
        .route("/api/git/branch", post(git_routes::branch_handler))
        .route("/api/git/diff", post(git_routes::diff_handler))
        .route("/api/git/commit", post(git_routes::commit_handler))
        .route("/api/git/push", post(git_routes::push_handler))
        .route("/api/git/pull", post(git_routes::pull_handler))
        .route("/api/git/pr/create", post(git_routes::create_pr_handler))
        .route("/api/git/pr/review", post(git_routes::review_pr_handler))
        .route("/api/git/worktrees", post(git_routes::worktrees_handler))
        // ── Roadmap artifact (webshell-roadmap-rendering) ────────────────────
        .route("/api/roadmap", get(roadmap::roadmap_handler))
        // ── HITL inbox — GitHub PR review queue (webshell-hitl-inbox Phase 1) ─
        .route("/api/gitforest/hitl-search", get(hitl_search_handler))
        .route("/api/gitforest/pr-metadata", get(pr_metadata_handler))
        // ── Cockpit GitHub proxy (webshell-cockpit Phase 3) ──────────────────
        .route(
            "/api/github-proxy/commits/{owner}/{repo}/{sha}",
            get(commit_metadata_handler),
        )
        .route(
            "/api/github-proxy/pr/{owner}/{repo}/{num}/review",
            post(submit_pr_review_handler),
        )
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
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::IF_MATCH,
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
pub async fn run(
    config: Config,
    docker_capable: DockerCapability,
    preflight: PreflightReport,
) -> Result<(), ServerError> {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    let state = AppState::new(config, docker_capable, preflight);
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
    preflight: PreflightReport,
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
                let state = AppState::new(try_config, docker_capable, preflight);
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

// ── HITL inbox handlers ───────────────────────────────────────────────────────

/// `GET /api/gitforest/hitl-search` — open PRs review-requested from the
/// authenticated GitHub user across all HITL-tracked repos.
///
/// Requires `Authorization: Bearer <token>` or `la_session` cookie.
/// Returns an empty array when no GitHub PAT is configured — degrades gracefully.
/// Results are cached 60s server-side via [`AppState::hitl_search_cache`].
async fn hitl_search_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(token) = crate::github_token_store::load_github_pat() else {
        return (
            StatusCode::OK,
            Json(Vec::<crate::github_proxy::HitlSearchItem>::new()),
        )
            .into_response();
    };
    let client = reqwest::Client::new();
    match crate::github_proxy::fetch_hitl_search(&client, &token, &state.hitl_search_cache).await {
        Ok(items) => (StatusCode::OK, Json((*items).clone())).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "hitl-search fetch failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// `GET /api/gitforest/pr-metadata?owner=<owner>&repo=<repo>&number=<n>` —
/// detailed metadata for a single PR.
///
/// Validates `(owner, repo)` against the SSRF allowlist before any outbound call.
/// Requires `Authorization: Bearer <token>` or `la_session` cookie.
/// Returns 403 when the repo is not allowlisted, 502 on upstream API failure.
async fn pr_metadata_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    AxumQuery(params): AxumQuery<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(owner) = params.get("owner").cloned() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(repo) = params.get("repo").cloned() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(number_str) = params.get("number") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(pr_number) = number_str.parse::<u64>() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !crate::github_proxy::is_hitl_tracked(&owner, &repo) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(token) = crate::github_token_store::load_github_pat() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let client = reqwest::Client::new();
    match crate::github_proxy::fetch_pr_metadata(
        &client,
        &token,
        &state.pr_metadata_cache,
        &owner,
        &repo,
        pr_number,
    )
    .await
    {
        Ok(meta) => (StatusCode::OK, Json((*meta).clone())).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "pr-metadata fetch failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

// ── Cockpit GitHub proxy (webshell-cockpit Phase 3) ──────────────────────────

use axum::extract::Path as AxumPath;

/// `GET /api/github-proxy/commits/{owner}/{repo}/{sha}`
///
/// Returns commit metadata (`sha`, first-line message, author login, `committed_at`).
/// SSRF guard: `(owner, repo)` must be in `HITL_TRACKED_REPOS`.
async fn commit_metadata_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    AxumPath((owner, repo, sha)): AxumPath<(String, String, String)>,
) -> impl axum::response::IntoResponse {
    if !crate::github_proxy::is_hitl_tracked(&owner, &repo) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(token) = crate::github_token_store::load_github_pat() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let client = reqwest::Client::new();
    match crate::github_proxy::fetch_commit_metadata(
        &client,
        &token,
        &state.commit_metadata_cache,
        &owner,
        &repo,
        &sha,
    )
    .await
    {
        Ok(meta) => (StatusCode::OK, Json((*meta).clone())).into_response(),
        Err(e) if e.starts_with("403") => StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "commit-metadata fetch failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

#[derive(serde::Deserialize)]
struct PrReviewBody {
    event: crate::github_proxy::PrReviewEvent,
    body: String,
}

/// `POST /api/github-proxy/pr/{owner}/{repo}/{num}/review`
///
/// Submits a GitHub PR review. Security controls:
/// - `If-Match: "<head_sha>"` header → 412 on SHA mismatch (replay defense).
/// - `Origin` header → 403 for non-allowlisted origins (CSRF).
/// - SSRF allowlist via `is_hitl_tracked`.
async fn submit_pr_review_handler(
    _: auth::AuthGuard,
    headers: axum::http::HeaderMap,
    AxumPath((owner, repo, pr_num)): AxumPath<(String, String, u64)>,
    Json(payload): Json<PrReviewBody>,
) -> impl axum::response::IntoResponse {
    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_matches('"').to_string());
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let Some(token) = crate::github_token_store::load_github_pat() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let client = reqwest::Client::new();
    let params = crate::github_proxy::PrReviewParams {
        pr_number: pr_num,
        event: payload.event,
        body: payload.body,
        if_match_sha: if_match.as_deref(),
        request_origin: origin.as_deref(),
    };
    match crate::github_proxy::submit_pr_review(&client, &token, &owner, &repo, params).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) if e.starts_with("412") => StatusCode::PRECONDITION_FAILED.into_response(),
        Err(e) if e.starts_with("403") => StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "pr-review submit failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
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
    _: auth::AuthGuard,
    State(state): State<AppState>,
    AxumQuery(params): AxumQuery<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
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

/// `GET /api/preflight` — structured infrastructure readiness probe (unauthenticated).
///
/// Always returns 200 regardless of `overall` status. The body carries the full
/// structured report with per-check results.
///
/// Monitoring systems MUST NOT restart the process on `overall: Blocked` alone —
/// a Blocked system is expected to be human-resolved (e.g. missing keychain entry).
/// `/api/health` is the binary liveness probe; this endpoint is the readiness probe.
async fn preflight_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let report = state.preflight.read().await.clone();
    (StatusCode::OK, Json(report))
}

/// `POST /api/preflight/refresh` — re-runs all 12 checks and updates the stored report.
///
/// Requires `Authorization: Bearer <token>`. Rate-limited to 1 request per 10 seconds
/// to prevent macOS keychain ACL dialog spam when the keychain is locked.
async fn preflight_refresh_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> axum::response::Response {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let last = state.preflight_last_refresh.load(Ordering::Relaxed);
    if now_secs.saturating_sub(last) < 10 {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    state
        .preflight_last_refresh
        .store(now_secs, Ordering::Relaxed);

    let agent_snap = state.active_agent.read().await.clone();
    let docker = state.docker_capable;
    let basic = preflight::run_basic().await;
    let report = preflight::run_full(&agent_snap, docker, basic).await;

    *state.preflight.write().await = report.clone();
    (StatusCode::OK, Json(report)).into_response()
}

/// `GET /api/auth-check` — validates either `Authorization: Bearer <token>`
/// **or** a valid `la_session` cookie via [`auth::AuthGuard`].
///
/// Responds 200 on match, 401 on mismatch or missing credentials.
async fn auth_check(_: auth::AuthGuard) -> impl IntoResponse {
    StatusCode::OK
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

/// `POST /api/auth/nonce` — issue a one-time auth nonce (gateway-to-webshell).
///
/// The gateway calls this immediately after the webshell starts up, passing its
/// resolved bearer token in `Authorization: Bearer <token>`. Returns
/// `{"nonce":"<uuid>"}` with a 60-second TTL. The nonce replaces the raw
/// bearer token in the launch URL so the real token never appears in MCP
/// tool-response logs (`~/.claude/projects/*/session.jsonl`).
async fn auth_issue_nonce(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let authorized = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| auth::validate_bearer(s, &state.config.token));
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Prune expired entries before inserting so the map stays bounded.
    let now = std::time::Instant::now();
    state.auth_nonces.retain(|_, exp| now <= *exp);
    let nonce = Uuid::new_v4();
    let expiry = now + std::time::Duration::from_secs(60);
    state.auth_nonces.insert(nonce, expiry);
    axum::Json(serde_json::json!({"nonce": nonce.to_string()})).into_response()
}

/// Request body for `POST /api/auth/nonce-exchange`.
#[derive(serde::Deserialize)]
struct NonceExchange {
    /// The one-time nonce UUID issued by `POST /api/auth/nonce`.
    nonce: String,
}

/// `POST /api/auth/nonce-exchange` — redeem a one-time nonce for a session cookie.
///
/// The browser calls this after reading `#nonce=<uuid>` from the URL fragment.
/// The nonce is deleted on first use and rejected if the 60-second TTL has elapsed.
async fn auth_nonce_exchange(
    State(state): State<AppState>,
    Json(body): Json<NonceExchange>,
) -> impl IntoResponse {
    let Ok(nonce_id) = body.nonce.parse::<Uuid>() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some((_, expiry)) = state.auth_nonces.remove(&nonce_id) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if std::time::Instant::now() > expiry {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let cookie = auth::session_cookie_header(&state.config.token);
    match HeaderValue::from_str(&cookie) {
        Ok(cookie_val) => {
            tracing::info!(target: "webshell", "Cookie session established via nonce exchange");
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
/// Authenticated via [`auth::AuthGuard`] (Bearer header **or** `la_session` cookie).
async fn read_browser_state(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let snapshot = state.browser_state.read().await;
    Json(snapshot.clone()).into_response()
}

/// `POST /api/browser-state` — updates the browser UI state snapshot.
///
/// Called periodically by the frontend to report current viewport, panel
/// sizes, zoom level, etc. Authenticated via [`auth::AuthGuard`]
/// (Bearer header **or** `la_session` cookie).
async fn write_browser_state(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    Json(update): Json<BrowserStateSnapshot>,
) -> impl IntoResponse {
    let mut snapshot = state.browser_state.write().await;
    *snapshot = update;

    StatusCode::OK
}

/// `GET /api/builds/resume` — returns all persisted sessions from `SessionStore`.
///
/// Auth-gated (global Bearer token). Results are ordered by `updated_at`
/// descending (most recently touched first).
async fn resume_sessions_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
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
async fn polytopes(_: auth::AuthGuard) -> impl IntoResponse {
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
