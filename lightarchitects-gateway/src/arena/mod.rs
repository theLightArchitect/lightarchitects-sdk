//! Arena — autonomous multi-agent research platform.
//!
//! Runs as `lightarchitects serve` — HTTP API + scheduler + heartbeat agents.
//! Manages sibling agents that produce intelligence briefs via Ollama,
//! posting to Discord/Telegram and persisting to the SOUL vault.
//!
//! # Entry Points
//!
//! - `run_serve` — Orchestrator mode: HTTP API + scheduler + spawn agent processes
//! - `run_agent` — Single-agent mode: one sibling's heartbeat loop

pub mod agent_loop;
pub mod alerting;
pub mod arena_config;
pub mod auth;
pub mod backend;
pub mod compat;
pub mod conductor;
pub mod conversation_routine;
pub mod grounding;
pub mod heartbeat;
pub mod llm;
pub mod mcp_pool;
pub mod rate_limit;
pub mod routes;
pub mod scheduler;
pub mod supervisor;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use tokio::sync::watch;

/// Shared application state for the Arena HTTP server.
///
/// Passed to Axum handlers via `State<Arc<AppState>>`.
pub struct AppState {
    /// MCP binary pool — immutable after startup, no outer lock needed.
    pub pool: Arc<mcp_pool::McpPool>,
    /// Arena configuration.
    pub config: arena_config::Config,
    /// HTTP request counter (monotonic, for logging/metrics).
    pub request_counter: std::sync::atomic::AtomicU64,
    /// API key auth store (optional — `None` if `ARENA_PEPPER` not set).
    pub auth_store: Option<Arc<auth::AuthStore>>,
    /// Per-endpoint rate limiter.
    pub rate_limiter: Arc<rate_limit::RateLimiter>,
    /// Supervisor handle for health queries.
    pub supervisor: Arc<supervisor::SupervisorHandle>,
    /// Shutdown signal — `true` means shutting down.
    pub shutdown_tx: watch::Sender<bool>,
}

/// Maximum request body size (1 MB) — enforced before JSON parsing.
const MAX_BODY_SIZE: usize = 1_048_576;

/// Agent names for the multi-agent architecture.
const AGENT_NAMES: &[&str] = &["eva", "corso", "quantum", "seraph", "ayin", "laex"];

// ── Public entry points ──────────────────────────────────────────────────

/// Run the Arena in orchestrator mode — HTTP API + scheduler + agent processes.
///
/// This is the `lightarchitects serve` entrypoint.
///
/// # Errors
///
/// Returns an error if config loading, MCP pool init, or HTTP binding fails.
pub async fn run_serve() -> Result<(), Box<dyn std::error::Error>> {
    let no_spawn =
        std::env::args().any(|a| a == "--no-spawn") || std::env::var("ARENA_DOCKER").is_ok();

    tracing::info!("Arena orchestrator starting");

    let config = arena_config::Config::from_env()?;
    let listen_addr = config.listen_addr;
    let pid_path = config.data_dir.join("arena.pid");
    write_pid_file(&pid_path);

    let auth_store = init_auth_store(&config)?;
    let rate_limiter =
        rate_limit::RateLimiter::new(config.rate_limit_window_secs, config.rate_limit_default);
    let pool = Arc::new(init_mcp_pool(&config).await?);

    let alerter = alerting::Alerter::new(
        config
            .telegram_bot_token
            .as_ref()
            .map(|t| t.expose_secret().to_owned()),
        config.telegram_chat_id.clone(),
        config.alert_threshold,
    );

    let supervisor_handle = supervisor::spawn(Arc::clone(&pool), alerter);
    tracing::info!("Process supervisor started");

    crate::channels::discord::spawn(
        config
            .discord_bot_token
            .as_ref()
            .map(|t| t.expose_secret().to_owned()),
    );

    let channels = Arc::new(crate::channels::Channels::from_env()?);
    let config_arc = Arc::new(config.clone());
    scheduler::spawn(
        Arc::clone(&pool),
        Arc::clone(&supervisor_handle),
        Arc::clone(&channels),
        &config,
        Arc::clone(&config_arc),
    );
    tracing::info!("Routine scheduler started");

    // Spawn helix significance watcher for canon-evaluation trigger
    let (spike_tx, spike_rx) = tokio::sync::mpsc::channel(32);
    let helix_root = dirs_next::home_dir()
        .map(|h| h.join(".soul/helix"))
        .unwrap_or_else(|| PathBuf::from("/root/.soul/helix"));
    match conversation_routine::spawn_helix_watcher(
        helix_root,
        config.significance_spike_threshold,
        spike_tx,
    ) {
        Ok(()) => tracing::info!("Helix significance watcher started"),
        Err(e) => tracing::warn!(error = %e, "Helix significance watcher failed to start"),
    }
    scheduler::spawn_spike_handler(spike_rx, Arc::clone(&config_arc));

    // Spawn agent processes (skip in Docker/--no-spawn mode)
    let agent_backend = backend::create_backend(&config);
    let agent_config =
        backend::AgentConfig::from_config(&config).map_err(|e| format!("AgentConfig: {e}"))?;

    let _managed_agents = if no_spawn {
        tracing::info!("Agent spawning disabled (Docker/--no-spawn mode)");
        backend::ManagedAgents::new()
    } else {
        spawn_all_agents(AGENT_NAMES, &agent_backend, &agent_config).await
    };

    tracing::info!(agents = AGENT_NAMES.len(), "Agent processes spawned");

    let (shutdown_tx, _shutdown_rx) = watch::channel(false);
    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        request_counter: std::sync::atomic::AtomicU64::new(1),
        auth_store,
        rate_limiter: Arc::clone(&rate_limiter),
        supervisor: supervisor_handle,
        shutdown_tx: shutdown_tx.clone(),
    });

    let app = build_router(Arc::clone(&state), &config.cors_origins);

    tracing::info!(%listen_addr, "Arena listening");
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await?;

    let _ = std::fs::remove_file(&pid_path);
    tracing::info!("Arena orchestrator shutdown complete");
    Ok(())
}

/// Run a single agent's heartbeat loop — one sibling's persistent context.
///
/// This is the `lightarchitects --agent <name>` entrypoint.
///
/// # Errors
///
/// Returns an error if config loading or MCP pool init fails.
pub async fn run_agent(agent_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(agent = %agent_name, "Agent starting");

    let config = arena_config::Config::from_env()?;
    let channels = Arc::new(crate::channels::Channels::from_env()?);
    let llm_client = Arc::new(llm::LlmClient::from_env()?);

    // Agent mode: only SOUL MCP server (reduces per-agent processes 5→1)
    let pool = Arc::new(init_agent_mcp_pool(&config).await?);
    let supervisor_handle = supervisor::spawn(Arc::clone(&pool), None);

    heartbeat::run_single_agent(
        agent_name,
        &config.data_dir,
        &llm_client,
        &channels,
        &supervisor_handle,
        &pool,
    )
    .await;

    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────

async fn spawn_all_agents(
    agent_names: &[&str],
    backend_impl: &Arc<dyn backend::AgentBackend>,
    agent_config: &backend::AgentConfig,
) -> backend::ManagedAgents {
    let managed = backend::ManagedAgents::new();
    for &name in agent_names {
        match backend_impl.spawn(name, agent_config).await {
            Ok(handle) => {
                tracing::info!(agent = %name, identity = ?handle.identity, "Agent spawned");
                managed.insert(handle).await;
            }
            Err(e) => {
                tracing::error!(agent = %name, error = %e, "Failed to spawn agent");
            }
        }
    }
    managed
}

fn write_pid_file(path: &PathBuf) {
    if let Err(e) = std::fs::write(path, std::process::id().to_string()) {
        tracing::warn!(path = %path.display(), error = %e, "Failed to write PID file");
    }
}

fn init_auth_store(
    config: &arena_config::Config,
) -> Result<Option<Arc<auth::AuthStore>>, Box<dyn std::error::Error>> {
    let Some(pepper) = &config.pepper else {
        tracing::error!("ARENA_PEPPER not set — all non-exempt requests will be denied");
        return Ok(None);
    };
    if let Some(dir) = config.db_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let db_path = config.db_path.to_string_lossy().to_string();
    let store = auth::AuthStore::new(&db_path, pepper.clone())?;
    tracing::info!(db = %db_path, "Auth store initialized");
    Ok(Some(Arc::new(store)))
}

async fn init_agent_mcp_pool(
    config: &arena_config::Config,
) -> Result<mcp_pool::McpPool, Box<dyn std::error::Error>> {
    let mut paths = HashMap::new();
    paths.insert("soul".into(), config.siblings.soul.clone());

    let mut pool = mcp_pool::McpPool::new(paths);
    pool.spawn_all().await?;
    tracing::info!("Agent MCP pool initialized (SOUL only)");
    Ok(pool)
}

async fn init_mcp_pool(
    config: &arena_config::Config,
) -> Result<mcp_pool::McpPool, Box<dyn std::error::Error>> {
    let mut paths = HashMap::new();
    paths.insert("corso".into(), config.siblings.corso.clone());
    paths.insert("eva".into(), config.siblings.eva.clone());
    paths.insert("soul".into(), config.siblings.soul.clone());
    paths.insert("quantum".into(), config.siblings.quantum.clone());
    paths.insert("seraph".into(), config.siblings.seraph.clone());
    if let Some(ref laex_path) = config.siblings.laex {
        paths.insert("laex".into(), laex_path.clone());
    }

    let mut pool = mcp_pool::McpPool::new(paths);
    pool.spawn_all().await?;
    tracing::info!("MCP binary pool initialized");
    Ok(pool)
}

fn build_router(state: Arc<AppState>, cors_origins: &[String]) -> axum::Router {
    use axum::extract::DefaultBodyLimit;
    use tower_http::cors::{AllowOrigin, CorsLayer};

    let cors = if cors_origins.iter().any(|o| o == "*") {
        CorsLayer::permissive()
    } else {
        let parsed: Vec<_> = cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(parsed))
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ])
    };

    axum::Router::new()
        .route(
            "/v1/{sibling}/{action}",
            axum::routing::post(routes::rest_action),
        )
        .route("/mcp", axum::routing::post(routes::mcp_post))
        .route("/v1/keys", axum::routing::post(routes::key_create))
        .route("/v1/larc/chat", axum::routing::post(routes::larc_chat))
        .route("/health", axum::routing::get(routes::health))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            rate_limit::rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            auth::auth_middleware,
        ))
        .layer(cors)
        .with_state(state)
}

async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    use tokio::signal::unix::{SignalKind, signal};
    let sigterm = signal(SignalKind::terminate()).ok();
    let sigint = signal(SignalKind::interrupt()).ok();
    match (sigterm, sigint) {
        (Some(mut st), Some(mut si)) => tokio::select! {
            _ = st.recv() => tracing::info!("Received SIGTERM"),
            _ = si.recv() => tracing::info!("Received SIGINT"),
        },
        (Some(mut st), None) => {
            st.recv().await;
        }
        (None, Some(mut si)) => {
            si.recv().await;
        }
        (None, None) => std::future::pending::<()>().await,
    }
    let _ = shutdown_tx.send(true);
}
