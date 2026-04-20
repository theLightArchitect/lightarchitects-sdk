//! Phase 19b.2 — Cross-sibling strand convergence detector.
//!
//! Runs as a background tokio task spawned from [`AppState::new`]. Every
//! [`POLL_INTERVAL`] the detector queries Neo4j for `:HotMemo` nodes whose
//! TTL hasn't expired, groups them by strand, and emits a
//! [`WebEvent::StrandConvergence`] for every strand whose distinct-sibling
//! count is at or above [`MIN_PARTICIPANTS`].
//!
//! Idempotency: a per-process `HashSet` records the `(strand, sorted_siblings)`
//! signatures already emitted so repeated polls don't re-fire the same
//! convergence. The set never shrinks — a sibling drop-out followed by a
//! re-join is intentionally silent (the convergence already happened). The
//! set does reset on process restart, which is the right coarse boundary for
//! Phase 19b's "signal when the world changes" semantic.
//!
//! Graph-side materialization of the convergence (a `:SharedExperience`
//! node + `:PARTICIPATES_IN` edges) is deferred to Phase 19c / 20 — this
//! phase ships the *detection* surface only so the UI can light up first.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use lightarchitects::helix::HelixDb;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::events::types::{StrandConvergenceEvent, WebEvent};
use crate::memory::persistence::SoulPersistence;

/// How often the detector polls Neo4j for new convergences.
///
/// 60 seconds is a pragmatic default — short enough that a 3-sibling
/// alignment surfaces in the UI within a minute of happening, long enough
/// that the polling cost is rounding error (one indexed lookup per minute).
const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Minimum distinct-sibling count for a strand to count as a convergence.
///
/// Three is the canonical cross-squad "resonance" threshold — when Claude,
/// CORSO, and EVA all land on the same strand, that's meaningful. Two is
/// noise (pairs align constantly); four+ is rare and still gets caught by
/// the ≥3 gate.
const MIN_PARTICIPANTS: usize = 3;

/// Spawn the convergence detector task. Returns immediately.
///
/// No-ops when Neo4j isn't attached. Surviving errors (Bolt failures,
/// timeout) are logged at `warn` and the next poll is attempted on
/// schedule — the detector never panics.
pub fn spawn(soul: Arc<SoulPersistence>, event_tx: broadcast::Sender<WebEvent>) {
    tokio::spawn(async move {
        let mut seen: HashSet<String> = HashSet::new();
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        // Skip the spurious first-tick-at-zero so the detector doesn't fire
        // before Neo4j has a chance to accept its first HotMemo write.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(msg) = poll_once(&soul, &event_tx, &mut seen).await {
                debug!(target: "soul.convergence", reason = %msg, "poll skipped");
            }
        }
    });
}

/// Single poll cycle — query Neo4j, filter by threshold, emit new events.
async fn poll_once(
    soul: &SoulPersistence,
    event_tx: &broadcast::Sender<WebEvent>,
    seen: &mut HashSet<String>,
) -> Result<(), String> {
    let Some(neo4j) = soul.neo4j_arc().await else {
        return Err("Neo4j tier unavailable".to_owned());
    };
    let db = neo4j.helix_db();

    // UNWIND each HotMemo's strands, group by strand name, collect the
    // distinct siblings + participating memo ids. The TTL gate keeps the
    // detector honest — expired memos don't contribute to convergence.
    let cypher = "MATCH (h:HotMemo) \
         WHERE h.expires > datetime() AND size(h.strands) > 0 \
         UNWIND h.strands AS strand \
         WITH strand, \
              collect(DISTINCT h.sibling) AS siblings, \
              collect(h.id) AS memo_ids \
         WHERE size(siblings) >= $min_participants \
         RETURN strand, siblings, memo_ids";
    let mut params = std::collections::BTreeMap::new();
    // MIN_PARTICIPANTS is a compile-time `const usize = 3`; serde_json
    // takes integer parameters as i64 so we convert via `i64::try_from`
    // which is infallible for any realistic MIN_PARTICIPANTS value.
    let min_participants: i64 = i64::try_from(MIN_PARTICIPANTS).unwrap_or(3);
    params.insert(
        "min_participants".into(),
        serde_json::json!(min_participants),
    );

    let records = db
        .execute_cypher_with_params(cypher, params)
        .await
        .map_err(|e| format!("convergence query failed: {e}"))?;

    let mut emitted = 0usize;
    for r in records {
        let Some(strand) = r.get("strand").and_then(|v| v.as_str()) else {
            continue;
        };
        let siblings = r
            .get("siblings")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let memo_ids = r
            .get("memo_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if siblings.len() < MIN_PARTICIPANTS {
            continue;
        }

        let signature = signature_for(strand, &siblings);
        if seen.contains(&signature) {
            continue;
        }
        seen.insert(signature.clone());

        // Phase 19c.1 — also materialize this convergence as a
        // :SharedExperience node in the graph so downstream queries
        // (Phase 13.3 convergences tab, /api/soul/convergences) can
        // read it via the normal SharedExperience surface. The id is
        // derived from the signature so re-runs MERGE-dedup across
        // process restarts — no duplicate :SharedExperience emitted
        // for the same (strand, sibling-set) tuple.
        if let Err(e) =
            materialize_convergence(db.as_ref(), &signature, strand, &siblings, &memo_ids).await
        {
            warn!(
                target: "soul.convergence",
                error = %e,
                signature = %signature,
                "materialize_convergence failed — SSE event still sent"
            );
        }

        let event = StrandConvergenceEvent {
            strand: strand.to_owned(),
            siblings,
            memo_ids,
            detected_at: Utc::now().to_rfc3339(),
        };
        if let Err(e) = event_tx.send(WebEvent::StrandConvergence(event)) {
            // Send error only happens when there are zero subscribers — the
            // Phase-18B pattern.
            warn!(target: "soul.convergence", error = %e, "no SSE subscribers for convergence");
        }
        emitted += 1;
    }

    if emitted > 0 {
        info!(
            target: "soul.convergence",
            emitted,
            tracked = seen.len(),
            "convergence poll"
        );
    }
    Ok(())
}

/// Deterministic signature for a `(strand, siblings)` pair — used for
/// idempotency. Siblings are sorted so `{a,b,c}` and `{c,b,a}` share a
/// signature.
fn signature_for(strand: &str, siblings: &[String]) -> String {
    let mut sorted: Vec<&str> = siblings.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    format!("{strand}::{}", sorted.join(","))
}

/// Phase 19c.1 — MERGE a `:SharedExperience` node for this convergence and
/// link each participating `:HotMemo` via `PARTICIPATES_IN`.
///
/// `id` is derived from the signature (prefixed with `se-conv-`) so
/// re-runs MERGE-dedup across process restarts — the detector's
/// in-memory `seen` set handles per-process dedup, this handles
/// cross-process.
///
/// `weight` encodes participation strength: `sibling_count / 7.0` clamped
/// to `[0.0, 1.0]`. Three-sibling convergence scores ~0.43; a full
/// seven-sibling squad-wide convergence scores 1.0.
///
/// `discovered_by` is set to `declared` — the closest existing variant
/// to a rule-based cross-sibling strand detector (Louvain is topology-
/// based and `embedding_ann` is similarity-based).
async fn materialize_convergence(
    db: &dyn HelixDb,
    signature: &str,
    strand: &str,
    siblings: &[String],
    memo_ids: &[String],
) -> Result<(), String> {
    let se_id = format!("se-conv-{}", stable_slug(signature));
    let participant_count = siblings.len();
    #[allow(clippy::cast_precision_loss)]
    let weight = (participant_count as f64 / 7.0).clamp(0.0, 1.0);
    let label = format!("Strand convergence: {strand} ({participant_count} siblings)");

    let cypher = "MERGE (se:SharedExperience {id: $se_id}) \
         ON CREATE SET se.weight = $weight, \
                       se.participant_count = $count, \
                       se.discovered_by = 'declared', \
                       se.label = $label, \
                       se.created_at = datetime($created_at) \
         WITH se \
         UNWIND $memo_ids AS mid \
         MATCH (h:HotMemo {id: mid}) \
         MERGE (h)-[:PARTICIPATES_IN]->(se)";

    let memo_ids_json: Vec<serde_json::Value> =
        memo_ids.iter().map(|m| serde_json::json!(m)).collect();
    let mut params = std::collections::BTreeMap::new();
    params.insert("se_id".into(), serde_json::json!(se_id));
    params.insert("weight".into(), serde_json::json!(weight));
    let count_i64 = i64::try_from(participant_count).unwrap_or(i64::MAX);
    params.insert("count".into(), serde_json::json!(count_i64));
    params.insert("label".into(), serde_json::json!(label));
    params.insert(
        "created_at".into(),
        serde_json::json!(Utc::now().to_rfc3339()),
    );
    params.insert("memo_ids".into(), serde_json::json!(memo_ids_json));

    db.execute_cypher_with_params(cypher, params)
        .await
        .map(|_| ())
        .map_err(|e| format!("cypher: {e}"))
}

/// Deterministic slug for a `:SharedExperience.id` derived from a
/// convergence signature. Replaces non-alphanumerics with `-` so the id
/// stays readable (no escape-hell in Neo4j filters) while preserving
/// signature→id bijection within the limits of a URL-safe charset.
fn stable_slug(signature: &str) -> String {
    signature
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_sibling_order_independent() {
        let a = signature_for(
            "analytical",
            &["corso".into(), "eva".into(), "webshell".into()],
        );
        let b = signature_for(
            "analytical",
            &["webshell".into(), "corso".into(), "eva".into()],
        );
        assert_eq!(a, b, "sibling order must not affect signature");
    }

    #[test]
    fn signatures_differ_by_strand() {
        let a = signature_for(
            "analytical",
            &["corso".into(), "eva".into(), "webshell".into()],
        );
        let b = signature_for(
            "methodical",
            &["corso".into(), "eva".into(), "webshell".into()],
        );
        assert_ne!(a, b);
    }

    #[test]
    fn signatures_differ_by_participant_set() {
        let a = signature_for(
            "analytical",
            &["corso".into(), "eva".into(), "webshell".into()],
        );
        let b = signature_for(
            "analytical",
            &["corso".into(), "eva".into(), "seraph".into()],
        );
        assert_ne!(a, b);
    }

    #[test]
    fn min_participants_is_three() {
        assert_eq!(MIN_PARTICIPANTS, 3);
    }

    #[test]
    fn stable_slug_preserves_alphanumerics() {
        assert_eq!(
            stable_slug("analytical::corso,eva,webshell"),
            "analytical--corso-eva-webshell"
        );
    }

    #[test]
    fn stable_slug_is_deterministic() {
        // Same input → same slug across runs.
        assert_eq!(stable_slug("x::a,b,c"), stable_slug("x::a,b,c"));
    }
}
