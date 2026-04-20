//! Hot-reloadable promotion policy — YAML config for significance thresholds.
//!
//! The policy is loaded from
//! `~/lightarchitects/soul/config/promotion-policy.yaml` at startup and
//! automatically reloaded within ~5 s of any write, without a restart.
//!
//! A [`PolicyWatcher`] subscribes to the parent directory with `notify`.
//! Internally it updates a shared [`PolicyHandle`] (`Arc<RwLock<PromotionPolicy>>`)
//! in-place, so all promotion calls always read the latest floor values via a
//! single cheap `RwLock::read()`.
//!
//! # Graceful degradation
//!
//! If the YAML file is absent, unreadable, or malformed, the policy silently
//! falls back to [`PromotionPolicy::default()`] (global floor = 7.0, no
//! per-sibling overrides).  Hot-reload is disabled when the `notify` watcher
//! fails to initialise — promotion still works, just with the initial snapshot.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;

use crate::turnlog::promotion::SIGNIFICANCE_AUTO_FLOOR;

// ── Data types ────────────────────────────────────────────────────────────────

/// Per-sibling promotion policy overrides.
///
/// Missing fields inherit from the global [`PromotionPolicy`].
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct SiblingPolicy {
    /// Override the global significance floor for this sibling.
    ///
    /// `None` means "use the global [`PromotionPolicy::significance_auto_floor`]".
    pub significance_auto_floor: Option<f64>,
    /// Entry kinds that always promote for this sibling regardless of
    /// significance (e.g. `["security_event"]` for SERAPH).
    pub always_promote_kinds: Option<Vec<String>>,
}

/// Promotion policy — loaded from YAML and hot-reloaded on file change.
///
/// # YAML example
///
/// ```yaml
/// significance_auto_floor: 7.0
/// per_sibling_overrides:
///   corso: { significance_auto_floor: 7.5 }
///   eva:   { significance_auto_floor: 6.5 }
///   seraph: { always_promote_kinds: [security_event] }
/// convergence_threshold: 3
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PromotionPolicy {
    /// Global significance floor.
    ///
    /// Entries whose metadata declares `significance >= significance_auto_floor`
    /// auto-promote.  Matches the Builders Cookbook ≥ 7.0 "enrich" threshold.
    pub significance_auto_floor: f64,
    /// Per-sibling threshold overrides keyed by sibling name.
    #[serde(default)]
    pub per_sibling_overrides: HashMap<String, SiblingPolicy>,
    /// Minimum number of convergent signals required for cross-sibling promotion
    /// (reserved for future use).
    pub convergence_threshold: Option<u32>,
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self {
            significance_auto_floor: SIGNIFICANCE_AUTO_FLOOR,
            per_sibling_overrides: HashMap::new(),
            convergence_threshold: None,
        }
    }
}

impl PromotionPolicy {
    /// Effective significance floor for `sibling`.
    ///
    /// Returns the per-sibling override when one is configured, otherwise falls
    /// back to [`Self::significance_auto_floor`].
    #[must_use]
    pub fn floor_for(&self, sibling: &str) -> f64 {
        self.per_sibling_overrides
            .get(sibling)
            .and_then(|s| s.significance_auto_floor)
            .unwrap_or(self.significance_auto_floor)
    }
}

/// Shared, hot-reloadable handle to the active [`PromotionPolicy`].
///
/// Clone is cheap — all clones share the same backing `RwLock`.
pub type PolicyHandle = Arc<RwLock<PromotionPolicy>>;

// ── PolicyWatcher ─────────────────────────────────────────────────────────────

/// Keeps a [`PolicyHandle`] up-to-date as the YAML policy file changes.
///
/// Drop this struct to stop the file-system subscription.
pub struct PolicyWatcher {
    _watcher: Option<RecommendedWatcher>,
}

impl PolicyWatcher {
    /// Spawn a watcher for `path`.
    ///
    /// Returns a [`PolicyHandle`] pre-loaded with the current file contents
    /// (or a default policy when the file is absent / invalid), and a
    /// `PolicyWatcher` that keeps the subscription alive.
    ///
    /// The parent directory is watched (not the file itself) to handle
    /// atomic rename-based writes common on macOS FSEvents.
    #[must_use]
    pub fn spawn(path: &Path) -> (PolicyHandle, Self) {
        // Own the path once; closures and watchers all capture the owned copy.
        let path = path.to_path_buf();
        let initial = load_policy(&path);
        let handle: PolicyHandle = Arc::new(RwLock::new(initial));

        let handle_clone = Arc::clone(&handle);
        let path_for_callback = path.clone();

        let watcher_result: Result<RecommendedWatcher, notify::Error> =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else { return };

                // Only react to create/modify events for our specific file.
                let is_our_file = event.paths.iter().any(|p| p == &path_for_callback);
                if !is_our_file {
                    return;
                }

                if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    return;
                }

                let new_policy = load_policy(&path_for_callback);
                match handle_clone.write() {
                    Ok(mut guard) => {
                        *guard = new_policy;
                        tracing::info!(
                            target: "turnlog::policy",
                            path = %path_for_callback.display(),
                            "promotion-policy.yaml reloaded"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "turnlog::policy",
                            error = %e,
                            "Failed to acquire policy write lock — keeping stale policy"
                        );
                    }
                }
            });

        let watcher = match watcher_result {
            Ok(mut w) => {
                let watch_dir = path
                    .parent()
                    .map_or_else(|| path.clone(), Path::to_path_buf);
                if let Err(e) = w.watch(&watch_dir, RecursiveMode::NonRecursive) {
                    tracing::warn!(
                        target: "turnlog::policy",
                        error = %e,
                        dir = %watch_dir.display(),
                        "Failed to watch policy directory — hot-reload disabled"
                    );
                }
                Some(w)
            }
            Err(e) => {
                tracing::warn!(
                    target: "turnlog::policy",
                    error = %e,
                    "Failed to create policy file watcher — hot-reload disabled"
                );
                None
            }
        };

        (handle, Self { _watcher: watcher })
    }

    /// Canonical policy file path:
    /// `~/lightarchitects/soul/config/promotion-policy.yaml`.
    ///
    /// Returns `None` when the `soul` runtime directory cannot be resolved.
    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        crate::core::paths::soul().map(|soul| soul.join("config").join("promotion-policy.yaml"))
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Load and parse the policy YAML; fall back to `Default` on any error.
fn load_policy(path: &Path) -> PromotionPolicy {
    match std::fs::read_to_string(path) {
        Err(_) => PromotionPolicy::default(),
        Ok(text) => match serde_yaml::from_str::<PromotionPolicy>(&text) {
            Ok(policy) => policy,
            Err(e) => {
                tracing::warn!(
                    target: "turnlog::policy",
                    path = %path.display(),
                    error = %e,
                    "Failed to parse promotion-policy.yaml — using defaults"
                );
                PromotionPolicy::default()
            }
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_matches_significance_auto_floor() {
        let policy = PromotionPolicy::default();
        assert!(
            (policy.significance_auto_floor - SIGNIFICANCE_AUTO_FLOOR).abs() < f64::EPSILON,
            "default floor must match SIGNIFICANCE_AUTO_FLOOR constant"
        );
    }

    #[test]
    fn floor_for_returns_global_when_no_override() {
        let policy = PromotionPolicy::default();
        assert!((policy.floor_for("corso") - SIGNIFICANCE_AUTO_FLOOR).abs() < f64::EPSILON);
    }

    #[test]
    fn floor_for_returns_override_when_present() {
        let policy: PromotionPolicy = serde_yaml::from_str(
            "significance_auto_floor: 7.0\nper_sibling_overrides:\n  corso:\n    significance_auto_floor: 7.5\n",
        )
        .unwrap();
        assert!((policy.floor_for("corso") - 7.5).abs() < f64::EPSILON);
        assert!((policy.floor_for("eva") - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn floor_for_eva_looser_override() {
        let policy: PromotionPolicy = serde_yaml::from_str(
            "significance_auto_floor: 7.0\nper_sibling_overrides:\n  eva:\n    significance_auto_floor: 6.5\n",
        )
        .unwrap();
        assert!((policy.floor_for("eva") - 6.5).abs() < f64::EPSILON);
        assert!((policy.floor_for("seraph") - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn load_policy_uses_default_for_absent_file() {
        let path = PathBuf::from("/nonexistent/path/policy.yaml");
        let policy = load_policy(&path);
        assert!((policy.significance_auto_floor - SIGNIFICANCE_AUTO_FLOOR).abs() < f64::EPSILON);
    }

    #[test]
    fn load_policy_parses_valid_yaml() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("policy.yaml");
        std::fs::write(
            &path,
            b"significance_auto_floor: 8.0\nper_sibling_overrides: {}\n",
        )
        .expect("write policy yaml");
        let policy = load_policy(&path);
        assert!((policy.significance_auto_floor - 8.0).abs() < f64::EPSILON);
    }
}
