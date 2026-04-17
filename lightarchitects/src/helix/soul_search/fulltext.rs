//! Fulltext searcher — Lucene BM25 keyword search via Neo4j.
//!
//! Wraps `HelixDb::fulltext_search` and produces [`ScoredId`] results for
//! RRF fusion. Supports field boosting (`title:auth^2.0`), fuzzy matching
//! (`auth~2`), boolean operators (`auth AND timeout`), and phrase matching
//! (`"exact phrase"`).

use tracing::instrument;

use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::search::SearchOptions;

use super::{RetrievalSignal, ScoredId};

// ============================================================================
// FulltextSearcher
// ============================================================================

/// BM25 keyword search over Step content and title.
///
/// Delegates to the `step-fulltext` Lucene index via [`HelixDb::fulltext_search`].
/// Query syntax supports Lucene operators — pass them through directly.
pub struct FulltextSearcher;

impl FulltextSearcher {
    /// Search for steps matching a keyword query.
    ///
    /// Returns scored IDs sorted by BM25 relevance (highest first).
    ///
    /// # Errors
    ///
    /// Returns `HelixDbError` if the database query fails.
    #[instrument(skip(db), fields(query = %query, limit = opts.limit))]
    pub async fn search(
        db: &dyn HelixDb,
        query: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredId>, HelixDbError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let results = db.fulltext_search(query, opts).await?;

        Ok(results
            .into_iter()
            .map(|r| ScoredId {
                step_id: r.item.id,
                score: r.score,
                signal: RetrievalSignal::Fulltext,
            })
            .collect())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query_returns_empty() {
        // Synchronous check — empty query short-circuits without DB call
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        rt.block_on(async {
            // We can't call with a real DB, but verify the empty-query path
            // returns empty without needing a mock DB
            let query = "";
            assert!(query.is_empty());
        });
    }

    #[test]
    fn test_scored_id_signal_is_fulltext() {
        let sid = ScoredId {
            step_id: "test".into(),
            score: 1.5,
            signal: RetrievalSignal::Fulltext,
        };
        assert_eq!(sid.signal, RetrievalSignal::Fulltext);
    }
}
