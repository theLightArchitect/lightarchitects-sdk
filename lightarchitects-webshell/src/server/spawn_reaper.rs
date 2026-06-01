//! Background reaper for orphaned agent containers.
//!
//! Runs every 10 seconds. Reconciles the in-memory `active_containers` registry
//! against `docker ps` to detect containers that exited without signalling the
//! relay (e.g. daemon restart, SIGKILL).
//!
//! # Kind-aware grace periods
//!
//! | Kind | Minimum age before reap | Rationale |
//! |------|------------------------|-----------|
//! | `Pty` | 15 s | Container may still be connecting its relay WebSocket |
//! | `WorkerTask` | 0 s | Task containers have no relay; immediate cleanup on exit |

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::container::types::{ActiveContainerEntry, ContainerKind};

/// Age threshold for PTY containers below which entries are not considered orphaned.
///
/// PTY containers are exempt from reaping for this window — they may be in-flight
/// between `docker run` success and the relay WebSocket being established.
const PTY_MIN_AGE_FOR_REAP: Duration = Duration::from_secs(15);

/// Reconciliation interval.
const REAP_INTERVAL: Duration = Duration::from_secs(10);

/// Spawn the background reaper task.
///
/// Takes shared refs to `active_containers` and the concurrent-cap `semaphore`
/// so orphan eviction returns slots to the pool.
#[allow(clippy::implicit_hasher)]
pub fn spawn(
    active_containers: Arc<std::sync::RwLock<HashMap<String, ActiveContainerEntry>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REAP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            reconcile(&active_containers, &semaphore).await;
            cleanup_exited().await;
        }
    });
}

/// Compares `active_containers` to running Docker containers; evicts stale entries.
///
/// Grace periods are kind-aware:
/// - [`ContainerKind::Pty`] — skip containers younger than [`PTY_MIN_AGE_FOR_REAP`].
/// - [`ContainerKind::WorkerTask`] — no minimum age; reap as soon as exit is detected.
async fn reconcile(
    active_containers: &Arc<std::sync::RwLock<HashMap<String, ActiveContainerEntry>>>,
    semaphore: &Arc<tokio::sync::Semaphore>,
) {
    // Snapshot currently tracked IDs and entry metadata.
    let tracked: Vec<(String, ContainerKind, Instant)> = active_containers
        .read()
        .map(|g| {
            g.iter()
                .map(|(k, e)| (k.clone(), e.kind.clone(), e.started_at))
                .collect()
        })
        .unwrap_or_default();

    if tracked.is_empty() {
        return;
    }

    let running_ids =
        crate::container::docker_cmd::ps_running_with_label("managed-by=la-hitl").await;
    let running_set: std::collections::HashSet<&str> =
        running_ids.iter().map(String::as_str).collect();

    let now = Instant::now();
    let mut stale: Vec<String> = Vec::new();

    for (id, kind, spawned_at) in &tracked {
        // Kind-aware grace period: PTY containers get a 15s window; WorkerTask containers
        // have no grace period — they should be reaped immediately once exited.
        let grace = match kind {
            ContainerKind::Pty => PTY_MIN_AGE_FOR_REAP,
            ContainerKind::WorkerTask { .. } => Duration::ZERO,
        };
        if now.duration_since(*spawned_at) < grace {
            continue;
        }
        if !running_set.contains(id.as_str()) {
            stale.push(id.clone());
        }
    }

    if stale.is_empty() {
        return;
    }

    tracing::warn!(
        count = stale.len(),
        ids = ?stale,
        "reaper detected orphaned containers — evicting and returning semaphore slots"
    );

    if let Ok(mut guard) = active_containers.write() {
        for id in &stale {
            guard.remove(id);
        }
    }
    // Return one semaphore slot per evicted container (paired with permit.forget()).
    semaphore.add_permits(stale.len());
}

/// Removes exited containers labelled `managed-by=la-hitl`.
async fn cleanup_exited() {
    let ids = crate::container::docker_cmd::ps_exited_with_label("managed-by=la-hitl").await;
    if ids.is_empty() {
        return;
    }
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    crate::container::docker_cmd::rm_force(&id_refs).await;
    tracing::info!(
        count = ids.len(),
        "reaper removed exited la-hitl containers"
    );
}
