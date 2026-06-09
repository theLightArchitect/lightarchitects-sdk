//! `CanonSource` — loads canon docs from `$HELIX/user/standards/canon/{doc}.md`,
//! slices a section by anchor prefix, returns truncated content.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use moka::future::Cache;
use tokio::time::timeout;

use super::super::catalog::ContextSource;
use super::section_slicer::slice_by_anchor_prefix;
use super::{ContextError, ContextResolver, ResolvedContext};

const DEFAULT_TTL: Duration = Duration::from_secs(300);
const DEFAULT_PER_CALL_TIMEOUT: Duration = Duration::from_millis(200);
const TOKEN_CHARS: usize = 4;

/// Maximum doc-name length (regex `[a-z][a-z0-9_-]{0,63}`).
const MAX_DOC_NAME: usize = 64;

/// Reads canon docs from `helix_root/user/standards/canon/{doc}.md` and
/// extracts an anchor-prefixed section.
pub struct CanonSource {
    helix_root: PathBuf,
    cache: Cache<String, Arc<String>>,
    per_call_timeout: Duration,
}

impl CanonSource {
    /// Construct with default 5-min TTL + 200 ms per-call timeout.
    #[must_use]
    pub fn new(helix_root: PathBuf) -> Self {
        Self::with_config(helix_root, DEFAULT_TTL, DEFAULT_PER_CALL_TIMEOUT)
    }

    /// Construct with custom TTL and per-call timeout.
    #[must_use]
    pub fn with_config(helix_root: PathBuf, ttl: Duration, per_call_timeout: Duration) -> Self {
        let cache = Cache::builder().time_to_live(ttl).max_capacity(64).build();
        Self {
            helix_root,
            cache,
            per_call_timeout,
        }
    }

    async fn load_doc(&self, doc: &str) -> Result<Arc<String>, ContextError> {
        if let Some(cached) = self.cache.get(doc).await {
            return Ok(cached);
        }
        let path = self.canon_path(doc)?;
        let read = timeout(self.per_call_timeout, tokio::fs::read_to_string(&path))
            .await
            .map_err(|_| ContextError::Timeout)?
            .map_err(|e| ContextError::Backend(e.to_string()))?;
        let arc = Arc::new(read);
        self.cache.insert(doc.to_owned(), arc.clone()).await;
        Ok(arc)
    }

    /// Build + validate the canon doc path. Per Cookbook §63.P5 — final
    /// canonical path must be contained within `helix_root/user/standards/canon`.
    fn canon_path(&self, doc: &str) -> Result<PathBuf, ContextError> {
        if !valid_doc_name(doc) {
            return Err(ContextError::NotFound("invalid doc name".into()));
        }
        let canon_root = self.helix_root.join("user").join("standards").join("canon");
        let candidate = canon_root.join(format!("{doc}.md"));
        let canonical_root =
            std::fs::canonicalize(&canon_root).map_err(|e| ContextError::Backend(e.to_string()))?;
        let canonical_candidate = std::fs::canonicalize(&candidate)
            .map_err(|_| ContextError::NotFound("canon doc not found".into()))?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(ContextError::NotFound("doc escapes canon root".into()));
        }
        Ok(canonical_candidate)
    }
}

#[async_trait]
impl ContextResolver for CanonSource {
    fn kind(&self) -> &'static str {
        "canon"
    }

    async fn resolve(
        &self,
        source: &ContextSource,
        _sibling: &str,
    ) -> Result<ResolvedContext, ContextError> {
        let (doc, anchor, token_budget) = match source {
            ContextSource::Canon {
                doc,
                anchor,
                token_budget,
            } => (doc.as_str(), anchor.as_str(), *token_budget),
            other => {
                return Err(ContextError::KindMismatch {
                    resolver: "canon",
                    got: other.kind_str(),
                });
            }
        };

        let content = self.load_doc(doc).await?;
        let Some(section) = slice_by_anchor_prefix(&content, anchor) else {
            return Err(ContextError::NotFound(format!("anchor not found in {doc}")));
        };

        let truncated = truncate_to_budget(&section, token_budget);
        let token_count_estimate = truncated.len() / TOKEN_CHARS;
        Ok(ResolvedContext {
            kind: "canon",
            identifier: format!("canon:{doc}#{anchor}"),
            content: truncated,
            token_count_estimate,
        })
    }
}

/// Per Cookbook §63 untrusted-input pattern: doc names must match a strict
/// regex to block path-traversal attempts (`../`, absolute paths, etc.).
fn valid_doc_name(doc: &str) -> bool {
    if doc.is_empty() || doc.len() > MAX_DOC_NAME {
        return false;
    }
    let mut chars = doc.chars();
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
        let canon = root.join("user").join("standards").join("canon");
        fs::create_dir_all(&canon).unwrap();
        let doc = "\
# Cookbook\n\
\n\
## 7. Agentic Architecture Patterns\n\
clarity body\n\
multi-line\n\
\n\
## §63 Untrusted-Input Operational Patterns (P1–P5)\n\
untrusted input body\n\
";
        fs::write(canon.join("builders-cookbook.md"), doc).unwrap();
        root
    }

    fn canon_source_of(doc: &str, anchor: &str, token_budget: usize) -> ContextSource {
        ContextSource::Canon {
            doc: doc.to_owned(),
            anchor: anchor.to_owned(),
            token_budget,
        }
    }

    #[tokio::test]
    async fn resolves_canon_section_truncated_to_budget() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let cs = CanonSource::new(root);
        let src = canon_source_of("builders-cookbook", "§63 — Rust patterns", 1000);
        let resolved = cs.resolve(&src, "corso").await.unwrap();
        assert_eq!(resolved.kind, "canon");
        assert!(resolved.content.contains("untrusted input body"));
        assert!(resolved.identifier.contains("builders-cookbook"));
    }

    #[tokio::test]
    async fn wrong_source_kind_returns_kind_mismatch() {
        let tmp = TempDir::new().unwrap();
        let cs = CanonSource::new(tmp.path().to_path_buf());
        let helix = ContextSource::Helix {
            owner_scope: "owner".into(),
            limit: 3,
            token_budget: 100,
        };
        let err = cs.resolve(&helix, "corso").await.unwrap_err();
        assert!(matches!(
            err,
            ContextError::KindMismatch {
                resolver: "canon",
                got: "helix"
            }
        ));
    }

    #[tokio::test]
    async fn unknown_doc_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let cs = CanonSource::new(root);
        let src = canon_source_of("does-not-exist", "§1 — Whatever", 500);
        let err = cs.resolve(&src, "corso").await.unwrap_err();
        assert!(matches!(err, ContextError::NotFound(_)));
    }

    #[tokio::test]
    async fn missing_anchor_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let cs = CanonSource::new(root);
        let src = canon_source_of("builders-cookbook", "§999 — Bogus", 500);
        let err = cs.resolve(&src, "corso").await.unwrap_err();
        assert!(matches!(err, ContextError::NotFound(_)));
    }

    #[tokio::test]
    async fn caches_doc_within_ttl() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let cs = CanonSource::new(root.clone());
        let src1 = canon_source_of("builders-cookbook", "§63 — Rust patterns", 500);
        let src2 = canon_source_of("builders-cookbook", "7. — Agentic", 500);
        let _ = cs.resolve(&src1, "corso").await.unwrap();
        // Delete the file — second call must succeed from cache.
        fs::remove_file(
            root.join("user")
                .join("standards")
                .join("canon")
                .join("builders-cookbook.md"),
        )
        .unwrap();
        let r2 = cs.resolve(&src2, "corso").await.unwrap();
        assert!(r2.content.contains("clarity body"));
    }

    #[tokio::test]
    async fn path_traversal_in_doc_name_rejected() {
        let tmp = TempDir::new().unwrap();
        let root = build_helix(&tmp);
        let cs = CanonSource::new(root);
        for evil in ["../canon", "..", "/etc/passwd", "BUILDERS-COOKBOOK", ""] {
            let src = canon_source_of(evil, "§1 — Whatever", 100);
            let err = cs.resolve(&src, "corso").await.unwrap_err();
            assert!(
                matches!(err, ContextError::NotFound(_) | ContextError::Backend(_)),
                "evil doc {evil:?} must not succeed (got {err:?})"
            );
        }
    }

    #[test]
    fn valid_doc_name_accepts_real_names() {
        assert!(valid_doc_name("builders-cookbook"));
        assert!(valid_doc_name("security-guardrails"));
        assert!(valid_doc_name("agents-playbook"));
        assert!(valid_doc_name("northstar"));
    }

    #[test]
    fn valid_doc_name_rejects_evil_names() {
        assert!(!valid_doc_name(""));
        assert!(!valid_doc_name("../canon"));
        assert!(!valid_doc_name("/etc/passwd"));
        assert!(!valid_doc_name("BUILDERS-COOKBOOK"));
        assert!(!valid_doc_name("doc.md"));
        assert!(!valid_doc_name(&"a".repeat(65)));
    }
}
