//! Background reaper for orphaned agent containers.
//!
//! Runs every 10 seconds. Reconciles the in-memory `active_containers` registry
//! against `docker ps` to detect containers that exited without signalling the
//! relay (e.g. daemon restart, SIGKILL).
//!
//! A 15-second age guard (H4 fix) prevents the reaper from evicting containers
//! that are still in the process of connecting their relay WebSocket.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

/// Age threshold below which containers are not considered orphaned.
///
/// Containers younger than this skip reconciliation — they may be in-flight
/// between `docker run` success and the relay WebSocket being established.
const MIN_AGE_FOR_REAP: Duration = Duration::from_secs(15);

/// Reconciliation interval.
const REAP_INTERVAL: Duration = Duration::from_secs(10);

/// Spawn the background reaper task.
///
/// Takes shared refs to `active_containers` and the concurrent-cap `semaphore`
/// so orphan eviction returns slots to the pool.
#[allow(clippy::implicit_hasher)]
pub fn spawn(
    active_containers: Arc<std::sync::RwLock<HashMap<String, Instant>>>,
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
async fn reconcile(
    active_containers: &Arc<std::sync::RwLock<HashMap<String, Instant>>>,
    semaphore: &Arc<tokio::sync::Semaphore>,
) {
    // Snapshot currently tracked IDs and spawn times.
    let tracked: Vec<(String, Instant)> = active_containers
        .read()
        .map(|g| g.iter().map(|(k, v)| (k.clone(), *v)).collect())
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
    for (id, spawned_at) in &tracked {
        // H4 fix: skip containers younger than MIN_AGE_FOR_REAP.
        if now.duration_since(*spawned_at) < MIN_AGE_FOR_REAP {
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
