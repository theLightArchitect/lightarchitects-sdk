//! `BroadcastingPromoter` — wraps a [`HelixPromoter`] and emits a
//! [`WebEvent::SoulPromotion`] on every successful `Promoted` outcome.
//!
//! Enables the Svelte webshell to watch hot→cold memory transitions in real
//! time via the shared SSE stream (`/api/events`). Failed and declined
//! promotions do not emit events — the UI only moves a memo once it's durably
//! written to the cold tier.
//!
//! # Design
//!
//! This wrapper is deliberately SDK-adjacent: the SDK's [`SiblingPromoter`]
//! stays free of any webshell or SSE concerns. The webshell composes a
//! `BroadcastingPromoter<SiblingPromoter>` at session-close time to bolt on
//! the broadcast capability.

use std::future::Future;
use std::sync::Arc;

use chrono::{Duration, Utc};
use lightarchitects::helix::HelixDb;
use lightarchitects::turnlog::promotion::{
    HelixPromoter, PromotionCandidate, PromotionError, PromotionOutcome, PromotionReason,
};
use tokio::sync::broadcast;
use tracing::warn;

use crate::events::{WebEventV2, types::WebEvent};
use crate::memory::persistence::SoulPersistence;
use crate::memory::types::{MemoryTier, PromotionEvent};

/// Default TTL applied to the `:HotMemo` side of the `MATERIALIZED_FROM`
/// lineage edge written at promotion time. Mirrors Phase 18B's
/// `HOT_MEMO_TTL_SECS`. A hot memo still gets a TTL even though the Step it
/// materialised into is permanent — if the hot side ever expires out of
/// retrieval, the edge remains and Step lineage stays queryable.
const LINEAGE_HOT_TTL_SECS: i64 = 24 * 60 * 60;

/// Wrap any [`HelixPromoter`] to additionally emit
/// [`WebEvent::SoulPromotion`] on each successful promotion AND dual-write
/// the entry into `SOUL` `SQLite` (Phase 10.3) so queries from the `SOUL` `MCP`
/// plugin see it immediately.
pub struct BroadcastingPromoter<P: HelixPromoter> {
    inner: P,
    tx: broadcast::Sender<WebEventV2>,
    soul: Option<Arc<SoulPersistence>>,
}

impl<P: HelixPromoter> BroadcastingPromoter<P> {
    /// Wrap `inner` so that successful promotions additionally publish on `tx`.
    ///
    /// Dual-write to `SOUL` `SQLite` is disabled; use [`with_soul`](Self::with_soul)
    /// to enable it.
    #[must_use]
    pub fn new(inner: P, tx: broadcast::Sender<WebEventV2>) -> Self {
        Self {
            inner,
            tx,
            soul: None,
        }
    }

    /// Enable `SOUL` `SQLite` dual-write. After the filesystem helix entry is
    /// durably written, this wrapper will also parse the file and insert it
    /// into the `helix_entries` table. Failures are logged at WARN but never
    /// propagated — the filesystem write is the source of truth.
    #[must_use]
    pub fn with_soul(mut self, soul: Arc<SoulPersistence>) -> Self {
        self.soul = Some(soul);
        self
    }
}

impl<P: HelixPromoter> HelixPromoter for BroadcastingPromoter<P> {
    fn promote(
        &self,
        candidate: PromotionCandidate,
    ) -> impl Future<Output = Result<PromotionOutcome, PromotionError>> + Send {
        // Capture fields we need to build the event *before* transferring
        // ownership of `candidate` to the inner promoter.
        let memo_id = format!("{}:{}", candidate.session_id, candidate.entry.seq);
        let sibling = candidate.entry.span.actor.to_string();
        let significance = significance_from_reason(&candidate.reason);
        // Phase 19b — snapshot candidate fields needed for the MATERIALIZED_FROM
        // edge before ownership transfers to the inner promoter.
        let memo_content = extract_hot_content(&candidate);
        let memo_strands = extract_hot_strands(&candidate);
        let memo_created_at = candidate.entry.span.timestamp;
        let tx = self.tx.clone();
        let soul = self.soul.clone();

        let inner_future = self.inner.promote(candidate);

        async move {
            let outcome = inner_future.await;
            if let Ok(PromotionOutcome::Promoted { ref helix_path }) = outcome {
                let rel_path = helix_relative(helix_path);

                // Phase 10.3 — dual-write: parse the just-written .md file and
                // insert/upsert into SOUL SQLite so subsequent queries from
                // the SOUL MCP plugin see the entry without a filesystem
                // rescan. Best-effort: any failure is logged and swallowed.
                if let Some(soul) = soul.as_ref() {
                    if let Some(entry) = read_storage_entry(helix_path, &rel_path).await {
                        // `Ok(true)` = written; `Ok(false)` = SQLite not
                        // available (filesystem write still stands). Both
                        // success paths are no-ops; only Err needs a log.
                        if let Err(e) = soul.write_entry(&entry).await {
                            warn!(
                                target: "soul",
                                error = %e,
                                path = %rel_path,
                                "SOUL SQLite dual-write failed"
                            );
                        }
                    } else {
                        warn!(
                            target: "soul",
                            path = %rel_path,
                            "dual-write: couldn't read newly-written file"
                        );
                    }

                    // Phase 19b.1 — write the MATERIALIZED_FROM lineage edge
                    // on the graph side. Non-fatal: the filesystem + SQLite
                    // dual-write above remains the source of truth. Edge
                    // write requires the Neo4j tier to be attached AND the
                    // promoted markdown to carry a UUID `id:` in its
                    // front-matter (the canonical path since Phase 9).
                    if let Some(step_id) = read_step_id_from_frontmatter(helix_path).await {
                        if let Some(neo4j) = soul.neo4j_arc().await {
                            let expires = memo_created_at + Duration::seconds(LINEAGE_HOT_TTL_SECS);
                            let params = build_lineage_params(
                                &step_id,
                                &memo_id,
                                &sibling,
                                &memo_content,
                                f64::from(significance),
                                &memo_strands,
                                memo_created_at,
                                expires,
                                &rel_path,
                            );
                            if let Err(e) = neo4j
                                .helix_db()
                                .execute_cypher_with_params(LINEAGE_CYPHER, params)
                                .await
                            {
                                warn!(
                                    target: "soul.lineage",
                                    error = %e,
                                    step_id = %step_id,
                                    memo_id = %memo_id,
                                    "MATERIALIZED_FROM edge write failed"
                                );
                            }
                        }
                    }
                }

                let event = PromotionEvent {
                    memo_id,
                    from: MemoryTier::Hot,
                    to: MemoryTier::Cold,
                    path: rel_path,
                    sibling,
                    significance,
                    promoted_at: Utc::now().to_rfc3339(),
                };
                // Failure to send means there are no live SSE subscribers — non-fatal.
                if let Err(broadcast::error::SendError(_)) =
                    tx.send(WebEventV2::from_event(WebEvent::SoulPromotion(event), None))
                {
                    warn!(
                        target: "webshell",
                        "SoulPromotion event had no listeners — SSE subscribers disconnected?"
                    );
                }
            }
            outcome
        }
    }
}

/// Parse a just-written helix markdown file into a [`StorageEntry`] for
/// `SQLite` dual-write. Returns `None` on I/O or parse failure — callers log
/// and degrade to filesystem-only.
async fn read_storage_entry(
    abs_path: &std::path::Path,
    rel_path: &str,
) -> Option<lightarchitects::soul::storage::StorageEntry> {
    use lightarchitects::soul::storage::StorageEntry;

    let raw = tokio::fs::read_to_string(abs_path).await.ok()?;
    let (fields, excerpt) = crate::memory::frontmatter::parse(&raw);

    // Derive sibling from the path's first segment if front-matter didn't supply it.
    let sibling_from_path = rel_path.split('/').next().unwrap_or("").to_owned();
    let sibling = fields.sibling.unwrap_or(sibling_from_path);

    // YAML significance is 0-10; StorageEntry matches that scale.
    let significance = fields
        .significance
        .map_or(0.0, |s| f64::from(s) * 10.0)
        .clamp(0.0, 10.0);

    // Body = raw minus the front-matter block; excerpt() trims this to 280.
    let body = excerpt.unwrap_or_default();

    let now = Utc::now();
    Some(StorageEntry {
        id: rel_path.to_owned(),
        path: rel_path.to_owned(),
        sibling,
        date: fields.created_at.as_deref().and_then(|s| {
            chrono::NaiveDate::parse_from_str(&s[..10.min(s.len())], "%Y-%m-%d").ok()
        }),
        entry_type: Some("experience".to_owned()),
        significance,
        self_defining: false,
        epoch: None,
        strands: fields.strands,
        resonance: Vec::new(),
        themes: Vec::new(),
        title: None,
        content: body,
        frontmatter: Some(fields.raw),
        created_at: now,
        updated_at: now,
    })
}

/// Compute the helix-relative path from an absolute promoted helix entry path.
///
/// Falls back to the full path if the helix root can't be resolved or the
/// absolute path isn't a prefix match — the frontend can accept either.
fn helix_relative(abs_path: &std::path::Path) -> String {
    let helix_root = lightarchitects::core::paths::helix_root_or_fallback();
    abs_path.strip_prefix(&helix_root).map_or_else(
        |_| abs_path.to_string_lossy().into_owned(),
        |p| p.to_string_lossy().into_owned(),
    )
}

/// Derive a [0.0, 1.0] significance from the promotion reason.
///
/// Normalises the SDK's 0–10 `SignificantReflection.weight` scale into the
/// `[0, 1]` range that matches the rest of the memory types.
#[allow(clippy::cast_possible_truncation)]
fn significance_from_reason(reason: &PromotionReason) -> f32 {
    let raw = match reason {
        PromotionReason::PausedMemo => 0.6,
        PromotionReason::SignificantReflection { weight } => (*weight / 10.0).clamp(0.0, 1.0),
        PromotionReason::UserFlagged => 0.75,
        // `AutoDetected` and any future non-exhaustive variants get a neutral
        // score — keeps the webshell compiling against SDK bumps.
        _ => 0.7,
    };
    raw as f32
}

// ── Phase 19b.1 — MATERIALIZED_FROM lineage edge helpers ──────────────────

/// Combined Cypher that MERGEs the `:HotMemo` node, ensures a minimal `:Step`
/// exists (the SOUL ingester will enrich its properties later), and then
/// MERGEs the `MATERIALIZED_FROM` edge between them.
///
/// All three statements run in a single Bolt round-trip so the invariant
/// "every promoted Step has a `MATERIALIZED_FROM` edge" holds atomically from
/// the graph's perspective — the edge cannot exist without both endpoints.
// Phase 18c Step 3 — ON CREATE SET extended with session_id / seq so promotion-
// created HotMemo nodes carry chain fields and don't confuse :NEXT edge matching.
// seq=0 is correct for promotion-materialised nodes (they have no seq in the
// NDJSON turnlog sense; they originate from the helix promotion pipeline).
const LINEAGE_CYPHER: &str = "\
    MERGE (h:HotMemo {id: $memo_id}) \
      ON CREATE SET h.sibling = $sibling, \
                    h.content = $content, \
                    h.significance = $significance, \
                    h.strands = $strands, \
                    h.created_at = datetime($created_at), \
                    h.expires = datetime($expires), \
                    h.session_id = $memo_id, \
                    h.seq = 0 \
    WITH h \
    MERGE (s:Step {id: $step_id}) \
      ON CREATE SET s.vault_path = $vault_path, \
                    s.content = $content, \
                    s.created_at = datetime($created_at) \
    MERGE (s)-[r:MATERIALIZED_FROM]->(h) \
      ON CREATE SET r.id = randomUUID() \
    RETURN r.id AS id";

/// Build the parameter map for [`LINEAGE_CYPHER`].
#[allow(clippy::too_many_arguments)]
fn build_lineage_params(
    step_id: &str,
    memo_id: &str,
    sibling: &str,
    content: &str,
    significance: f64,
    strands: &[String],
    created_at: chrono::DateTime<Utc>,
    expires: chrono::DateTime<Utc>,
    vault_path: &str,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut p = std::collections::BTreeMap::new();
    p.insert("memo_id".into(), serde_json::json!(memo_id));
    p.insert("step_id".into(), serde_json::json!(step_id));
    p.insert("sibling".into(), serde_json::json!(sibling));
    p.insert("content".into(), serde_json::json!(content));
    p.insert("significance".into(), serde_json::json!(significance));
    let strands_json: Vec<serde_json::Value> =
        strands.iter().map(|s| serde_json::json!(s)).collect();
    p.insert("strands".into(), serde_json::json!(strands_json));
    p.insert(
        "created_at".into(),
        serde_json::json!(created_at.to_rfc3339()),
    );
    p.insert("expires".into(), serde_json::json!(expires.to_rfc3339()));
    p.insert("vault_path".into(), serde_json::json!(vault_path));
    p
}

/// Parse the just-written promoted markdown file and extract the UUID from
/// its `id:` front-matter field. Returns `None` when the file is unreadable
/// or the YAML doesn't carry an `id`.
async fn read_step_id_from_frontmatter(abs_path: &std::path::Path) -> Option<String> {
    let raw = tokio::fs::read_to_string(abs_path).await.ok()?;
    let (fields, _) = crate::memory::frontmatter::parse(&raw);
    fields
        .raw
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// Short summary of the hot memo's content for the `:HotMemo.content`
/// property — prefers explicit metadata fields over the raw action name.
fn extract_hot_content(candidate: &PromotionCandidate) -> String {
    const MAX: usize = 280;
    let meta = &candidate.entry.span.metadata;
    for key in ["memo_body", "summary", "content", "message"] {
        if let Some(s) = meta.get(key).and_then(serde_json::Value::as_str) {
            let mut out: String = s.chars().take(MAX).collect();
            if s.chars().count() > MAX {
                out.push('…');
            }
            return out;
        }
    }
    format!("[{}]", candidate.entry.span.action)
}

/// Strand tags lifted off the span metadata, lowercased. Returns an empty
/// vec if absent.
fn extract_hot_strands(candidate: &PromotionCandidate) -> Vec<String> {
    candidate
        .entry
        .span
        .metadata
        .get("strands")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_lowercase))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]
mod tests {
    use super::*;
    use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
    use lightarchitects::turnlog::entry::TurnEntry;
    use std::path::PathBuf;

    /// A fake promoter that always returns `Promoted` at the given path.
    struct FakePromoter {
        path: PathBuf,
    }

    impl HelixPromoter for FakePromoter {
        fn promote(
            &self,
            _candidate: PromotionCandidate,
        ) -> impl Future<Output = Result<PromotionOutcome, PromotionError>> + Send {
            let path = self.path.clone();
            async move { Ok(PromotionOutcome::Promoted { helix_path: path }) }
        }
    }

    /// A fake promoter that always returns `Declined`.
    struct DecliningPromoter;
    impl HelixPromoter for DecliningPromoter {
        async fn promote(
            &self,
            _candidate: PromotionCandidate,
        ) -> Result<PromotionOutcome, PromotionError> {
            Ok(PromotionOutcome::Declined {
                reason: "below threshold".into(),
            })
        }
    }

    fn make_candidate() -> PromotionCandidate {
        let span = TraceContext::new(Actor::new("corso"), "reflection")
            .outcome(TraceOutcome::Continue)
            .finish()
            .unwrap();
        let entry = TurnEntry {
            seq: 7,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        };
        PromotionCandidate {
            entry,
            session_id: "sess-42".into(),
            project_root: PathBuf::from("/tmp"),
            reason: PromotionReason::SignificantReflection { weight: 8.0 },
            window: None,
        }
    }

    #[tokio::test]
    async fn promoted_emits_soul_promotion_event() {
        let (tx, mut rx) = broadcast::channel::<WebEventV2>(16);
        let inner = FakePromoter {
            path: PathBuf::from("/tmp/helix/corso/entries/x.md"),
        };
        let promoter = BroadcastingPromoter::new(inner, tx);

        let outcome = promoter.promote(make_candidate()).await.unwrap();
        assert!(matches!(outcome, PromotionOutcome::Promoted { .. }));

        let event = rx.recv().await.unwrap();
        match event {
            WebEventV2 {
                inner: WebEvent::SoulPromotion(pe),
                ..
            } => {
                assert_eq!(pe.memo_id, "sess-42:7");
                assert_eq!(pe.sibling, "corso");
                assert_eq!(pe.from, MemoryTier::Hot);
                assert_eq!(pe.to, MemoryTier::Cold);
                // significance comes from SignificantReflection { weight: 8.0 } → 0.8
                assert!((pe.significance - 0.8).abs() < 1e-3);
            }
            other => panic!("expected SoulPromotion, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn declined_does_not_emit_event() {
        let (tx, mut rx) = broadcast::channel::<WebEventV2>(16);
        let promoter = BroadcastingPromoter::new(DecliningPromoter, tx);
        promoter.promote(make_candidate()).await.unwrap();
        // Channel should be empty — try_recv returns Err(Empty).
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn significance_from_paused_memo() {
        assert!((significance_from_reason(&PromotionReason::PausedMemo) - 0.6).abs() < 1e-3);
    }

    #[test]
    fn significance_clamped_at_upper_bound() {
        let reason = PromotionReason::SignificantReflection { weight: 50.0 };
        assert_eq!(significance_from_reason(&reason), 1.0);
    }
}
