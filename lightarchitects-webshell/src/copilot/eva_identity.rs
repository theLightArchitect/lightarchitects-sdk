//! EVA identity loader — loads `eva/identity.md` from the helix root and
//! makes it available to the copilot prompt prelude as an `[Identity]` block.
//!
//! ## Design
//!
//! `EvaIdentityCache` holds the stripped identity text and the file's last
//! known `mtime`.  A background task (spawned in `AppState::new`) is the
//! **sole writer**: it checks the mtime on a 30-second interval, re-reads on
//! change, and acquires the write lock only to commit the update.  Per-request
//! reads acquire only a read lock — no file I/O on the hot path.

use std::path::Path;
use std::time::SystemTime;

/// Maximum identity size accepted into the prompt prelude.
const MAX_IDENTITY_BYTES: usize = 32 * 1024;

/// Loaded, frontmatter-stripped EVA identity text + mtime sentinel.
#[derive(Debug, Default, Clone)]
pub struct EvaIdentityCache {
    /// Frontmatter-stripped body of `eva/identity.md`, truncated to
    /// [`MAX_IDENTITY_BYTES`].  Empty string when the file is absent or
    /// unreadable.
    text: String,
    /// Last observed `mtime` of the identity file; `None` before first load.
    mtime: Option<SystemTime>,
}

impl EvaIdentityCache {
    /// Load the identity file at `path`, strip YAML frontmatter, and truncate
    /// to [`MAX_IDENTITY_BYTES`].  Returns an empty cache on any I/O error.
    pub fn load(path: &Path) -> Self {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "eva_identity: failed to read identity file — copilot continues without persona"
                );
                return Self::default();
            }
        };
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let body = strip_frontmatter(&raw);
        let text = if body.len() > MAX_IDENTITY_BYTES {
            tracing::warn!(
                bytes = body.len(),
                limit = MAX_IDENTITY_BYTES,
                "eva_identity: identity file exceeds 32 KiB — truncating"
            );
            body[..MAX_IDENTITY_BYTES].to_owned()
        } else {
            body.to_owned()
        };
        Self { text, mtime }
    }

    /// Return the stripped identity text (may be empty).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Check whether the file at `path` has changed since last load.  If it
    /// has, reload in-place and return `true`.  Returns `false` when the mtime
    /// is unchanged or unreadable (no-op, safe to call on every tick).
    pub fn check_reload(&mut self, path: &Path) -> bool {
        let current_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if current_mtime == self.mtime {
            return false;
        }
        *self = Self::load(path);
        true
    }
}

/// Strip YAML frontmatter delimited by `---` at the top of the file.
///
/// Returns the body after the closing `---`.  If no frontmatter is present,
/// returns the whole string.
fn strip_frontmatter(s: &str) -> &str {
    let trimmed = s.trim_start();
    if !trimmed.starts_with("---") {
        return s;
    }
    // Find the closing delimiter — skip the opening `---` line.
    let after_open = match trimmed.find('\n') {
        Some(n) => &trimmed[n + 1..],
        None => return s,
    };
    match after_open.find("\n---") {
        Some(close) => {
            let after_close = &after_open[close + 4..]; // skip "\n---"
            after_close.trim_start_matches('\n')
        }
        None => s,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_temp(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn identity_block_prepended() {
        let f = write_temp("hello eva");
        let cache = EvaIdentityCache::load(f.path());
        assert_eq!(cache.text(), "hello eva");
    }

    #[test]
    fn identity_skipped_when_empty() {
        let cache = EvaIdentityCache::default();
        assert!(cache.text().is_empty());
    }

    #[test]
    fn identity_truncated_at_32k() {
        let big = "x".repeat(MAX_IDENTITY_BYTES + 100);
        let f = write_temp(&big);
        let cache = EvaIdentityCache::load(f.path());
        assert_eq!(cache.text().len(), MAX_IDENTITY_BYTES);
    }

    #[test]
    fn frontmatter_stripped() {
        let content = "---\nname: EVA\n---\nThis is the body.\n";
        let f = write_temp(content);
        let cache = EvaIdentityCache::load(f.path());
        assert_eq!(cache.text(), "This is the body.\n");
    }

    #[test]
    fn hot_reload_on_mtime_change() {
        // Write version one, load it, then point at a new file (different mtime
        // sentinel) — check_reload must detect the change and reload.
        let f1 = write_temp("version one");
        let mut cache = EvaIdentityCache::load(f1.path());
        assert_eq!(cache.text(), "version one");

        let f2 = write_temp("version two");
        let reloaded = cache.check_reload(f2.path());
        assert!(
            reloaded,
            "check_reload should return true when mtime differs"
        );
        assert_eq!(cache.text(), "version two");
    }
}
