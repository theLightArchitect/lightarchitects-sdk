//! `IndustryBaselineSource` — loads industry-baseline files from
//! `$HELIX/user/standards/industry-baselines/{category}/{path}` and returns
//! truncated content.
//!
//! Unlike [`super::CanonSource`], baseline files are **whole-file** content
//! (no section slicing) — the catalog declares both the `category`
//! subdirectory and the `path` within it, and the resolver reads the file
//! end-to-end before truncating to `token_budget`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use moka::future::Cache;
use tokio::time::timeout;

use super::super::catalog::ContextSource;
use super::{ContextError, ContextResolver, ResolvedContext};

const DEFAULT_TTL: Duration = Duration::from_secs(300);
const DEFAULT_PER_CALL_TIMEOUT: Duration = Duration::from_millis(200);
const TOKEN_CHARS: usize = 4;
const MAX_CATEGORY: usize = 32;

/// Reads industry-baseline files from
/// `helix_root/user/standards/industry-baselines/{category}/{path}` and
/// returns truncated content.
pub struct IndustryBaselineSource {
    helix_root: PathBuf,
    cache: Cache<String, Arc<String>>,
    per_call_timeout: Duration,
}

impl IndustryBaselineSource {
    /// Construct with default 5-min TTL + 200 ms per-call timeout.
    #[must_use]
    pub fn new(helix_root: PathBuf) -> Self {
        Self::with_config(helix_root, DEFAULT_TTL, DEFAULT_PER_CALL_TIMEOUT)
    }

    /// Construct with custom TTL and per-call timeout.
    #[must_use]
    pub fn with_config(helix_root: PathBuf, ttl: Duration, per_call_timeout: Duration) -> Self {
        let cache = Cache::builder().time_to_live(ttl).max_capacity(256).build();
        Self {
            helix_root,
            cache,
            per_call_timeout,
        }
    }

    /// Build + validate the baseline path. Per Cookbook §63.P5 — final
    /// canonical path must be contained within
    /// `helix_root/user/standards/industry-baselines/{category}`.
    fn baseline_path(&self, category: &str, path: &str) -> Result<PathBuf, ContextError> {
        if !valid_category(category) {
            return Err(ContextError::NotFound("invalid category".into()));
        }
        if path.is_empty() || path.contains("..") || path.starts_with('/') {
            return Err(ContextError::NotFound("invalid path".into()));
        }
        let category_root = self
            .helix_root
            .join("user")
            .join("standards")
            .join("industry-baselines")
            .join(category);
        let candidate = category_root.join(path);
        let canonical_root = std::fs::canonicalize(&category_root)
            .map_err(|_| ContextError::NotFound("category not found".into()))?;
        let canonical_candidate = std::fs::canonicalize(&candidate)
            .map_err(|_| ContextError::NotFound("baseline file not found".into()))?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(ContextError::NotFound("path escapes baseline root".into()));
        }
        Ok(canonical_candidate)
    }

    /// Cache-first read: returns cached content if present; otherwise
    /// canonicalises + reads the file and inserts into the cache.
    async fn load_or_read(&self, category: &str, path: &str) -> Result<Arc<String>, ContextError> {
        let cache_key = format!("{category}/{path}");
        if let Some(cached) = self.cache.get(&cache_key).await {
            return Ok(cached);
        }
        let canonical = self.baseline_path(category, path)?;
        let read = timeout(self.per_call_timeout, tokio::fs::read_to_string(&canonical))
            .await
            .map_err(|_| ContextError::Timeout)?
            .map_err(|e| ContextError::Backend(e.to_string()))?;
        let arc = Arc::new(read);
        self.cache.insert(cache_key, arc.clone()).await;
        Ok(arc)
    }
}

#[async_trait]
impl ContextResolver for IndustryBaselineSource {
    fn kind(&self) -> &'static str {
        "industry-baseline"
    }

    async fn resolve(
        &self,
        source: &ContextSource,
        _sibling: &str,
    ) -> Result<ResolvedContext, ContextError> {
        let (category, path, token_budget) = match source {
            ContextSource::IndustryBaseline {
                category,
                path,
                token_budget,
            } => (category.as_str(), path.as_str(), *token_budget),
            other => {
                return Err(ContextError::KindMismatch {
                    resolver: "industry-baseline",
                    got: other.kind_str(),
                });
            }
        };
        let content = self.load_or_read(category, path).await?;
        let truncated = truncate_to_budget(&content, token_budget);
        let token_count_estimate = truncated.len() / TOKEN_CHARS;
        Ok(ResolvedContext {
            kind: "industry-baseline",
            identifier: format!("baseline:{category}/{path}"),
            content: truncated,
            token_count_estimate,
        })
    }
}

fn valid_category(category: &str) -> bool {
    if category.is_empty() || category.len() > MAX_CATEGORY {
        return false;
    }
    let mut chars = category.chars();
    let first = chars.next();
    if !first.is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn truncate_to_budget(s: &str, token_budget: usize) -> String {
    let char_budget = token_budget.saturating_mul(TOKEN_CHARS);
    if s.len() <= char_budget {
        return s.to_owned();
    }
    let mut out = s[..char_budget].to_owned();
    while !out.is_empty() && !out.is_char_boundary(out.len()) {
        out.pop();
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn build_helix(tmp: &TempDir) -> PathBuf {
        let root = tmp.path().to_path_buf();
        let cat = root
            .join("user")
            .join("standards")
            .join("industry-baselines")
            .join("security")
            .join("owasp");
        fs::create_dir_all(&cat).unwrap();
        fs::write(
            cat.join("owasp-llm-top-10.md"),
            "# OWASP LLM Top 10\n\nLLM01 prompt injection content.\n",
        )
        .unwrap();
        // Write a sentinel outside the baseline root to ensure traversal is rejected.
        fs::create_dir_all(root.join("secrets")).unwrap();
        fs::write(root.join("secrets").join("creds.txt"), "TOPSECRET").unwrap();
        root
    }

    fn baseline_source_of(category: &str, path: &str, token_budget: usize) -> ContextSource {
        ContextSource::IndustryBaseline {
            category: category.to_owned(),
            path: path.to_owned(),
            token_budget,
        }
    }

    #[tokio::test]
    async fn resolves_baseline_file_truncated_to_budget() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let bs = IndustryBaselineSource::new(root);
        let src = baseline_source_of("security", "owasp/owasp-llm-top-10.md", 500);
        let resolved = bs.resolve(&src, "corso").await.unwrap();
        assert_eq!(resolved.kind, "industry-baseline");
        assert!(resolved.content.contains("LLM01"));
        assert!(resolved.identifier.contains("security/owasp"));
    }

    #[tokio::test]
    async fn wrong_source_kind_returns_kind_mismatch() {
        let tmp = TempDir::new().unwrap();
        let bs = IndustryBaselineSource::new(tmp.path().to_path_buf());
        let canon = ContextSource::Canon {
            doc: "builders-cookbook".into(),
            anchor: "§63".into(),
            token_budget: 100,
        };
        let err = bs.resolve(&canon, "corso").await.unwrap_err();
        assert!(matches!(
            err,
            ContextError::KindMismatch {
                resolver: "industry-baseline",
                got: "canon"
            }
        ));
    }

    #[tokio::test]
    async fn missing_file_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let bs = IndustryBaselineSource::new(root);
        let src = baseline_source_of("security", "owasp/does-not-exist.md", 100);
        let err = bs.resolve(&src, "corso").await.unwrap_err();
        assert!(matches!(err, ContextError::NotFound(_)));
    }

    #[tokio::test]
    async fn path_traversal_attempt_rejected() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let bs = IndustryBaselineSource::new(root);
        for evil in [
            "../../secrets/creds.txt",
            "../../../etc/passwd",
            "/etc/passwd",
            "..",
        ] {
            let src = baseline_source_of("security", evil, 100);
            let err = bs.resolve(&src, "corso").await.unwrap_err();
            assert!(
                matches!(err, ContextError::NotFound(_)),
                "evil path {evil:?} must be rejected (got {err:?})"
            );
        }
    }

    #[tokio::test]
    async fn invalid_category_rejected() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let bs = IndustryBaselineSource::new(root);
        for evil in ["../", "SECURITY", "", "sec/urity"] {
            let src = baseline_source_of(evil, "f.md", 100);
            let err = bs.resolve(&src, "corso").await.unwrap_err();
            assert!(
                matches!(err, ContextError::NotFound(_)),
                "evil category {evil:?} must be rejected (got {err:?})"
            );
        }
    }

    #[tokio::test]
    async fn caches_file_within_ttl() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let bs = IndustryBaselineSource::new(root.clone());
        let src = baseline_source_of("security", "owasp/owasp-llm-top-10.md", 500);
        let _ = bs.resolve(&src, "corso").await.unwrap();
        // Delete; second call must succeed from cache.
        fs::remove_file(
            root.join("user")
                .join("standards")
                .join("industry-baselines")
                .join("security")
                .join("owasp")
                .join("owasp-llm-top-10.md"),
        )
        .unwrap();
        let r2 = bs.resolve(&src, "corso").await.unwrap();
        assert!(r2.content.contains("LLM01"));
    }

    #[test]
    fn valid_category_table() {
        assert!(valid_category("security"));
        assert!(valid_category("quality"));
        assert!(valid_category("operations"));
        assert!(valid_category("research"));
        assert!(!valid_category(""));
        assert!(!valid_category("SECURITY"));
        assert!(!valid_category("sec/urity"));
        assert!(!valid_category("../"));
        assert!(!valid_category(&"a".repeat(33)));
    }
}
