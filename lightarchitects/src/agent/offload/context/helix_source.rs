//! `HelixSource` — owner-scoped helix enricher with a 5-minute TTL cache and
//! per-call 200 ms timeout.
//!
//! # Cache
//!
//! Keyed on `format!("{owner}:{limit}")`. Uses [`moka::future::Cache`] in
//! line with the codebase standard (`helix/cache.rs:185-192`).
//!
//! # Production wiring (Day 13)
//!
//! ```ignore
//! use std::sync::Arc;
//! use async_trait::async_trait;
//! use lightarchitects::agent::offload::context::HelixQueryRunner;
//! use lightarchitects::helix::{HelixStore, HelixDbError, types::Step};
//!
//! struct StoreRunner(Arc<HelixStore>);
//!
//! #[async_trait]
//! impl HelixQueryRunner for StoreRunner {
//!     async fn fetch_by_owner(&self, owner: &str, limit: u32)
//!         -> Result<Vec<Step>, HelixDbError>
//!     {
//!         // helix_id format per Explore agent 2026-06-08: `{owner}/{owner}`
//!         let helix_id = format!("{owner}/{owner}");
//!         self.0.helix_db().get_steps(&helix_id, Some(limit), Some(0)).await
//!     }
//! }
//! ```
//!
//! Day 13 acceptance gate: `~/.claude/plans/moonlit-soaring-sifakis.md`.

use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use moka::future::Cache;
use tokio::time::timeout;

use crate::helix::types::Step;

use super::super::catalog::ContextSource;
use super::{ContextError, ContextResolver, HelixQueryRunner, ResolvedContext};

/// Default cache TTL — 5 minutes (matches `helix/cache.rs` codebase standard).
const DEFAULT_TTL: Duration = Duration::from_secs(300);

/// Default per-call deadline — 200 ms (per BUILD plan Risk #2).
const DEFAULT_PER_CALL_TIMEOUT: Duration = Duration::from_millis(200);

/// Chars per LLM token (rough heuristic — accurate within ±20% for English).
const TOKEN_CHARS: usize = 4;

/// Owner-scoped helix enricher.
///
/// Caches per `(owner, limit)` for [`DEFAULT_TTL`]; bounded per-call by
/// [`DEFAULT_PER_CALL_TIMEOUT`].
pub struct HelixSource {
    runner: Arc<dyn HelixQueryRunner>,
    cache: Cache<String, Vec<Step>>,
    per_call_timeout: Duration,
}

impl HelixSource {
    /// Construct with default TTL (5 min) + default per-call timeout (200 ms).
    #[must_use]
    pub fn new(runner: Arc<dyn HelixQueryRunner>) -> Self {
        Self::with_config(runner, DEFAULT_TTL, DEFAULT_PER_CALL_TIMEOUT)
    }

    /// Construct with custom TTL (preserves default per-call timeout).
    #[must_use]
    pub fn with_ttl(runner: Arc<dyn HelixQueryRunner>, ttl: Duration) -> Self {
        Self::with_config(runner, ttl, DEFAULT_PER_CALL_TIMEOUT)
    }

    /// Construct with custom TTL and per-call timeout.
    #[must_use]
    pub fn with_config(
        runner: Arc<dyn HelixQueryRunner>,
        ttl: Duration,
        per_call_timeout: Duration,
    ) -> Self {
        let cache = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(1_024)
            .build();
        Self {
            runner,
            cache,
            per_call_timeout,
        }
    }
}

#[async_trait]
impl ContextResolver for HelixSource {
    fn kind(&self) -> &'static str {
        "helix"
    }

    async fn resolve(
        &self,
        source: &ContextSource,
        sibling: &str,
    ) -> Result<ResolvedContext, ContextError> {
        let (owner_scope, limit, token_budget) = match source {
            ContextSource::Helix {
                owner_scope,
                limit,
                token_budget,
            } => (owner_scope.as_str(), *limit, *token_budget),
            other => {
                return Err(ContextError::KindMismatch {
                    resolver: "helix",
                    got: other.kind_str(),
                });
            }
        };

        // `owner_scope == "owner"` is the catalog default token — resolve to
        // the calling sibling at runtime.
        let owner: &str = if owner_scope == "owner" {
            sibling
        } else {
            owner_scope
        };
        let key = format!("{owner}:{limit}");

        let entries: Vec<Step> = if let Some(cached) = self.cache.get(&key).await {
            cached
        } else {
            let limit_u32 = u32::try_from(limit).unwrap_or(u32::MAX);
            let fetched = timeout(
                self.per_call_timeout,
                self.runner.fetch_by_owner(owner, limit_u32),
            )
            .await
            .map_err(|_| ContextError::Timeout)?
            .map_err(|e| ContextError::Backend(e.to_string()))?;
            self.cache.insert(key, fetched.clone()).await;
            fetched
        };

        let now = Utc::now();
        let live: Vec<&Step> = entries
            .iter()
            .filter(|s| s.expires.is_none_or(|exp| exp > now))
            .collect();
        let (content, token_count_estimate) = assemble_and_truncate(&live, token_budget);
        Ok(ResolvedContext {
            kind: "helix",
            identifier: format!("owner={owner} limit={limit}"),
            content,
            token_count_estimate,
        })
    }
}

/// Concatenate live steps as `## {title}\n{content}\n\n` blocks; truncate to
/// `token_budget * 4` chars on a UTF-8 code-point boundary.
///
/// Returns the assembled buffer + a (chars / 4) token-count estimate.
fn assemble_and_truncate(entries: &[&Step], token_budget: usize) -> (String, usize) {
    let mut buf = String::new();
    for step in entries {
        if let Some(t) = &step.title {
            let _ = writeln!(buf, "## {t}");
        }
        let _ = writeln!(buf, "{}", step.content);
        buf.push('\n');
    }
    let char_budget = token_budget.saturating_mul(TOKEN_CHARS);
    if buf.len() > char_budget {
        buf.truncate(char_budget);
        while !buf.is_empty() && !buf.is_char_boundary(buf.len()) {
            buf.pop();
        }
    }
    let token_estimate = buf.len() / TOKEN_CHARS;
    (buf, token_estimate)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    use chrono::{DateTime, Duration as ChronoDuration, Utc};
    use serde_json::Value;

    use crate::helix::HelixDbError;

    use super::*;

    /// In-memory mock recording call count, last-owner, last-limit, with an
    /// optional artificial delay.
    struct MockHelixRunner {
        call_count: AtomicUsize,
        last_owner: Mutex<Option<String>>,
        last_limit: AtomicU32,
        response: Mutex<Vec<Step>>,
        block_for: Option<Duration>,
    }

    impl MockHelixRunner {
        fn new(response: Vec<Step>) -> Self {
            Self {
                call_count: AtomicUsize::new(0),
                last_owner: Mutex::new(None),
                last_limit: AtomicU32::new(0),
                response: Mutex::new(response),
                block_for: None,
            }
        }

        fn with_delay(response: Vec<Step>, delay: Duration) -> Self {
            Self {
                call_count: AtomicUsize::new(0),
                last_owner: Mutex::new(None),
                last_limit: AtomicU32::new(0),
                response: Mutex::new(response),
                block_for: Some(delay),
            }
        }

        fn count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }

        fn last_owner(&self) -> Option<String> {
            self.last_owner.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl HelixQueryRunner for MockHelixRunner {
        async fn fetch_by_owner(&self, owner: &str, limit: u32) -> Result<Vec<Step>, HelixDbError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            *self.last_owner.lock().unwrap() = Some(owner.to_owned());
            self.last_limit.store(limit, Ordering::SeqCst);
            if let Some(d) = self.block_for {
                tokio::time::sleep(d).await;
            }
            let response = self.response.lock().unwrap().clone();
            Ok(response.into_iter().take(limit as usize).collect())
        }
    }

    fn fixture_step(
        id: &str,
        title: Option<&str>,
        content: &str,
        expires: Option<DateTime<Utc>>,
    ) -> Step {
        Step {
            id: id.to_owned(),
            helix_id: "test/test".to_owned(),
            title: title.map(str::to_owned),
            content: content.to_owned(),
            significance: 5.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires,
            created_at: Utc::now(),
            metadata: Value::Null,
            vault_path: None,
            graph_embedding: None,
        }
    }

    fn helix_source_of(owner_scope: &str, limit: usize, token_budget: usize) -> ContextSource {
        ContextSource::Helix {
            owner_scope: owner_scope.to_owned(),
            limit,
            token_budget,
        }
    }

    #[test]
    fn kind_returns_helix() {
        let runner = Arc::new(MockHelixRunner::new(vec![]));
        let src = HelixSource::new(runner);
        assert_eq!(src.kind(), "helix");
    }

    #[tokio::test]
    async fn wrong_source_kind_returns_kind_mismatch() {
        let runner = Arc::new(MockHelixRunner::new(vec![]));
        let src = HelixSource::new(runner);
        let canon = ContextSource::Canon {
            doc: "builders-cookbook".into(),
            anchor: "§7".into(),
            token_budget: 500,
        };
        let err = src.resolve(&canon, "corso").await.unwrap_err();
        assert!(matches!(
            err,
            ContextError::KindMismatch {
                resolver: "helix",
                got: "canon"
            }
        ));
    }

    #[tokio::test]
    async fn owner_scope_owner_resolves_to_caller_sibling() {
        let runner = Arc::new(MockHelixRunner::new(vec![fixture_step(
            "s1",
            Some("Hello"),
            "world",
            None,
        )]));
        let src = HelixSource::new(runner.clone());
        let s = helix_source_of("owner", 3, 100);
        let resolved = src.resolve(&s, "corso").await.unwrap();
        assert_eq!(runner.last_owner().as_deref(), Some("corso"));
        assert!(resolved.identifier.contains("owner=corso"));
        assert_eq!(resolved.kind, "helix");
    }

    #[tokio::test]
    async fn owner_scope_concrete_passes_through() {
        let runner = Arc::new(MockHelixRunner::new(vec![]));
        let src = HelixSource::new(runner.clone());
        let s = helix_source_of("soul", 3, 100);
        let _ = src.resolve(&s, "corso").await.unwrap();
        assert_eq!(runner.last_owner().as_deref(), Some("soul"));
    }

    #[tokio::test]
    async fn caches_within_ttl() {
        let runner = Arc::new(MockHelixRunner::new(vec![fixture_step(
            "s1", None, "x", None,
        )]));
        let src = HelixSource::new(runner.clone());
        let s = helix_source_of("corso", 3, 100);
        let _ = src.resolve(&s, "corso").await.unwrap();
        let _ = src.resolve(&s, "corso").await.unwrap();
        assert_eq!(runner.count(), 1, "second call must hit cache");
    }

    #[tokio::test]
    async fn recasts_after_ttl_expiry() {
        let runner = Arc::new(MockHelixRunner::new(vec![fixture_step(
            "s1", None, "x", None,
        )]));
        let src = HelixSource::with_ttl(runner.clone(), Duration::from_millis(50));
        let s = helix_source_of("corso", 3, 100);
        let _ = src.resolve(&s, "corso").await.unwrap();
        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = src.resolve(&s, "corso").await.unwrap();
        assert_eq!(runner.count(), 2, "post-TTL call must re-query backend");
    }

    #[tokio::test]
    async fn respects_token_budget() {
        // Build a step with 5000 chars; budget = 100 tokens → 400 chars cap.
        let big = "abcde".repeat(1000); // 5000 chars
        let runner = Arc::new(MockHelixRunner::new(vec![fixture_step(
            "s1", None, &big, None,
        )]));
        let src = HelixSource::new(runner);
        let s = helix_source_of("corso", 3, 100);
        let resolved = src.resolve(&s, "corso").await.unwrap();
        assert!(
            resolved.content.len() <= 400,
            "content length {} must be ≤ 400 chars",
            resolved.content.len()
        );
        // Truncation must end on a UTF-8 boundary.
        assert!(resolved.content.is_char_boundary(resolved.content.len()));
        assert_eq!(resolved.token_count_estimate, resolved.content.len() / 4);
    }

    #[tokio::test]
    async fn filters_expired_steps() {
        let past = Utc::now() - ChronoDuration::hours(1);
        let runner = Arc::new(MockHelixRunner::new(vec![
            fixture_step("live", Some("Live entry"), "live content", None),
            fixture_step("dead", Some("Expired entry"), "expired content", Some(past)),
        ]));
        let src = HelixSource::new(runner);
        let s = helix_source_of("corso", 5, 10_000);
        let resolved = src.resolve(&s, "corso").await.unwrap();
        assert!(resolved.content.contains("live content"));
        assert!(
            !resolved.content.contains("expired content"),
            "expired step must be filtered"
        );
    }

    #[tokio::test]
    async fn timeout_returns_error() {
        let runner = Arc::new(MockHelixRunner::with_delay(
            vec![fixture_step("s1", None, "x", None)],
            Duration::from_millis(500),
        ));
        let src =
            HelixSource::with_config(runner, Duration::from_secs(300), Duration::from_millis(50));
        let s = helix_source_of("corso", 3, 100);
        let err = src.resolve(&s, "corso").await.unwrap_err();
        assert!(matches!(err, ContextError::Timeout));
    }

    #[test]
    fn assemble_and_truncate_unit() {
        let steps = vec![
            fixture_step("a", Some("Title A"), "body a", None),
            fixture_step("b", None, "body b", None),
        ];
        let refs: Vec<&Step> = steps.iter().collect();
        let (content, est) = assemble_and_truncate(&refs, 1000);
        assert!(content.contains("## Title A"));
        assert!(content.contains("body a"));
        assert!(content.contains("body b"));
        assert_eq!(est, content.len() / 4);
    }

    #[tokio::test]
    async fn cache_key_includes_limit() {
        let runner = Arc::new(MockHelixRunner::new(vec![fixture_step(
            "s1", None, "x", None,
        )]));
        let src = HelixSource::new(runner.clone());
        let s3 = helix_source_of("corso", 3, 100);
        let s5 = helix_source_of("corso", 5, 100);
        let _ = src.resolve(&s3, "corso").await.unwrap();
        let _ = src.resolve(&s5, "corso").await.unwrap();
        assert_eq!(
            runner.count(),
            2,
            "limit=3 vs limit=5 must produce distinct cache keys"
        );
    }
}
