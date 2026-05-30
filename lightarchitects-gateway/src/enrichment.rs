//! Real-time helix enrichment worker.
//!
//! After any SOUL write action (`write_note`, `ingest`, `enrich`) completes,
//! a background tokio task projects newly embedded Step nodes to 128-dim
//! `sage_embedding` vectors via [`BgeSageProjectionPipeline`].
//!
//! # Activation
//!
//! Set `SOUL_ENRICH_ASYNC=true` in the environment.  When unset or when Neo4j
//! credentials are unavailable, [`maybe_enrich`] is a no-op and no connection
//! is opened.
//!
//! # Architecture
//!
//! A bounded `mpsc` channel carries unit signals (`()`) from [`maybe_enrich`]
//! to the background worker.  The worker calls
//! [`BgeSageProjectionPipeline::project_pending`] on each signal, projecting
//! up to [`PASS_LIMIT`] steps per wake-up.  Overflow signals are silently
//! dropped — the consolidator remains the safety net for full reconciliation.

use std::sync::{Arc, OnceLock};

use lightarchitects::helix::soul_search::BgeSageProjectionPipeline;
use lightarchitects::helix::{HelixDb, HelixNeo4j, Neo4jConfig, Neo4jConnectionMode};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// SOUL actions that trigger an enrichment pass.
const ENRICH_ACTIONS: &[&str] = &["write_note", "ingest", "enrich", "ingest_source"];

/// Bounded channel capacity — overflow signals are silently dropped.
const CHANNEL_CAPACITY: usize = 64;

/// Maximum steps to project per enrichment wake-up.
const PASS_LIMIT: usize = 50;

static ENRICH_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// Fire-and-forget enrichment signal after a completed SOUL write action.
///
/// No-op when the worker is not running (`SOUL_ENRICH_ASYNC` not set or
/// Neo4j unavailable) or when `agent_name` is not `"soul"` or `action` is
/// not a write action.  Never blocks — excess signals are silently dropped.
pub fn maybe_enrich(agent_name: &str, action: &str) {
    if agent_name != "soul" {
        return;
    }
    if !ENRICH_ACTIONS.contains(&action) {
        return;
    }
    if let Some(tx) = ENRICH_TX.get() {
        if tx.try_send(()).is_err() {
            debug!(action, "enrichment channel full — signal dropped");
        }
    }
}

/// Attempt to start the real-time enrichment background worker.
///
/// Reads Neo4j credentials from the macOS keychain (`soul-neo4j-local`) or
/// environment variables.  If `SOUL_ENRICH_ASYNC` is not `"true"` or
/// credentials are unavailable, this is a silent no-op.
///
/// Safe to call multiple times — only the first call initialises the channel.
pub async fn start_worker_if_enabled() {
    if std::env::var("SOUL_ENRICH_ASYNC").as_deref() != Ok("true") {
        return;
    }
    let Some((uri, user, password)) = read_neo4j_creds() else {
        warn!("SOUL_ENRICH_ASYNC=true but Neo4j credentials unavailable — enrichment disabled");
        return;
    };
    if let Err(e) = do_start(uri, user, password).await {
        warn!(error = %e, "SOUL_ENRICH_ASYNC: worker startup failed — enrichment disabled");
    }
}

/// Connect to Neo4j, initialise the channel, and spawn the worker task.
async fn do_start(uri: String, user: String, password: String) -> Result<(), String> {
    let db = HelixNeo4j::connect(&Neo4jConfig {
        uri: uri.clone(),
        user,
        password: secrecy::SecretString::from(password),
        mode: Neo4jConnectionMode::Local,
    })
    .await
    .map_err(|e| format!("Neo4j connect: {e}"))?;

    let db: Arc<dyn HelixDb> = Arc::new(db);
    let pipeline =
        BgeSageProjectionPipeline::load_or_default(&BgeSageProjectionPipeline::default_path());

    let (tx, rx) = mpsc::channel::<()>(CHANNEL_CAPACITY);
    if ENRICH_TX.set(tx).is_err() {
        // Another call beat us to it — not an error.
        return Ok(());
    }

    info!(neo4j_uri = %uri, "SOUL_ENRICH_ASYNC: enrichment worker started");
    tokio::spawn(run_worker(db, pipeline, rx));
    Ok(())
}

/// Background worker loop — project pending steps on each signal.
async fn run_worker(
    db: Arc<dyn HelixDb>,
    pipeline: BgeSageProjectionPipeline,
    mut rx: mpsc::Receiver<()>,
) {
    while rx.recv().await.is_some() {
        match pipeline.project_pending(db.as_ref(), PASS_LIMIT).await {
            Ok(0) => debug!("SOUL enrichment: no pending steps"),
            Ok(n) => info!(projected = n, "SOUL enrichment: projected pending steps"),
            Err(e) => warn!(error = %e, "SOUL enrichment: projection failed"),
        }
    }
    info!("SOUL enrichment worker shut down");
}

/// Read Neo4j credentials from keychain (macOS) or env vars.
fn read_neo4j_creds() -> Option<(String, String, String)> {
    let uri =
        keychain_read("soul-neo4j-local", "uri").or_else(|| std::env::var("NEO4J_URI").ok())?;
    let user = keychain_read("soul-neo4j-local", "username")
        .or_else(|| std::env::var("NEO4J_USER").ok())
        .unwrap_or_else(|| "neo4j".to_owned());
    let password = keychain_read("soul-neo4j-local", "password")
        .or_else(|| std::env::var("NEO4J_PASS").ok())?;
    Some((uri, user, password))
}

/// Read a keychain item via the macOS `security` CLI.
///
/// The `security` binary is in the keychain ACL and reads without a GUI
/// dialog — safe for ad-hoc-signed binaries.
#[cfg(target_os = "macos")]
fn keychain_read(service: &str, account: &str) -> Option<String> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8(out.stdout).ok()?;
        let trimmed = s.trim().to_owned();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn keychain_read(_service: &str, _account: &str) -> Option<String> {
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn maybe_enrich_ignores_non_soul_agents() {
        // Should not panic even without a worker initialised.
        maybe_enrich("corso", "write_note");
        maybe_enrich("eva", "ingest");
    }

    #[test]
    fn maybe_enrich_ignores_non_write_soul_actions() {
        maybe_enrich("soul", "query");
        maybe_enrich("soul", "search");
        maybe_enrich("soul", "helix");
    }

    #[test]
    fn maybe_enrich_noop_without_worker() {
        // No worker running — ENRICH_TX is None in tests. Should silently no-op.
        maybe_enrich("soul", "write_note");
        maybe_enrich("soul", "ingest");
        maybe_enrich("soul", "enrich");
    }
}
