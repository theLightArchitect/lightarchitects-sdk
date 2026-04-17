//! Filesystem watcher — helix vault fallback event source.
//!
//! Watches the helix root returned by [`lightarchitects_core::paths::helix_root`]
//! for new and modified `.md` files inside any `entries/` subdirectory, and
//! emits [`WebEvent::HelixEntry`] events on the shared broadcast channel.
//!
//! # When this runs
//!
//! The watcher is the **fallback** source: it activates regardless of whether
//! AYIN is reachable. When AYIN is running, the browser receives both
//! `ayin_span` events (richer trace metadata) and `helix_entry` events
//! (raw file signals). When AYIN is down, the watcher is the sole source.
//!
//! # Debounce
//!
//! Many editors write files in stages (temp → rename, or multiple `write`
//! syscalls).  The watcher debounces events per path: a second event for the
//! same path within [`DEBOUNCE_MS`] milliseconds is silently dropped.
//!
//! # Helix root missing
//!
//! If [`lightarchitects_core::paths::helix_root`] returns `None` (the vault
//! is not set up yet), [`HelixWatcher::spawn`] logs at `WARN` and returns
//! without panicking.  The SSE stream continues with AYIN-only events.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::broadcast;
use tracing::{info, warn};

use super::types::{BuildEventKind, BuildUpdateEvent, HelixEntrySummary, HelixEventKind, WebEvent};

/// Per-path debounce window in milliseconds.
const DEBOUNCE_MS: u64 = 500;

/// Evict stale debounce entries older than this.
const DEBOUNCE_EVICT_SECS: u64 = 60;

/// Manages the helix vault filesystem watcher.
pub struct HelixWatcher;

impl HelixWatcher {
    /// Spawns a blocking task that watches the helix root for vault entry changes.
    ///
    /// If the helix root path is unavailable (vault not configured), logs at
    /// `WARN` and returns without spawning.  Callers do not need to handle the
    /// unavailable case — the system degrades gracefully to AYIN-only events.
    pub fn spawn(tx: broadcast::Sender<WebEvent>) {
        let Some(root) = lightarchitects_core::paths::helix_root() else {
            warn!("helix_root unavailable — filesystem watcher not started");
            return;
        };
        drop(tokio::task::spawn_blocking(move || run_watcher(root, tx)));
    }
}

/// Blocking watcher loop.  Runs until the broadcast channel closes.
fn run_watcher(root: PathBuf, tx: broadcast::Sender<WebEvent>) {
    let (notify_tx, notify_rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();

    let Ok(mut watcher) = notify::recommended_watcher(move |res| {
        let _ = notify_tx.send(res);
    }) else {
        warn!("Failed to create filesystem watcher for helix root");
        return;
    };

    if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
        warn!(error = %e, path = %root.display(), "Failed to watch helix root");
        return;
    }

    info!(path = %root.display(), "Helix filesystem watcher active");

    let mut debounce: HashMap<PathBuf, Instant> = HashMap::new();

    for res in &notify_rx {
        match res {
            Ok(event) => {
                if !process_event(event, &root, &mut debounce, &tx) {
                    break; // all broadcast receivers dropped — stop watching
                }
            }
            Err(e) => warn!(error = %e, "Helix watcher error"),
        }
    }

    info!("Helix filesystem watcher stopped");
}

/// Processes one notify event; returns `false` when the broadcast channel closes.
fn process_event(
    event: notify::Event,
    root: &Path,
    debounce: &mut HashMap<PathBuf, Instant>,
    tx: &broadcast::Sender<WebEvent>,
) -> bool {
    let now = Instant::now();

    for path in event.paths {
        let event_kind = match event.kind {
            EventKind::Create(_) => (HelixEventKind::Created, BuildEventKind::Created),
            EventKind::Modify(_) => (HelixEventKind::Modified, BuildEventKind::Modified),
            _ => continue, // ignore access, remove, other events per path
        };

        // Per-path debounce: drop events within DEBOUNCE_MS of the last emit.
        if let Some(last) = debounce.get(&path) {
            if last.elapsed() < Duration::from_millis(DEBOUNCE_MS) {
                continue;
            }
        }
        debounce.insert(path.clone(), now);

        // Evict stale entries to bound memory usage.
        debounce.retain(|_, last| last.elapsed() < Duration::from_secs(DEBOUNCE_EVICT_SECS));

        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();

        // Route: build tracking files → BuildUpdate, vault entries → HelixEntry.
        if is_build_file(&path) {
            let entry = BuildUpdateEvent {
                path: rel_path,
                event_kind: event_kind.1,
            };
            if tx.send(WebEvent::BuildUpdate(entry)).is_err() {
                return false; // channel closed
            }
        } else if is_helix_entry(&path) {
            let entry = HelixEntrySummary {
                path: rel_path,
                event_kind: event_kind.0,
            };
            if tx.send(WebEvent::HelixEntry(entry)).is_err() {
                return false; // channel closed
            }
        }
        // Other file types are silently ignored.
    }

    true
}

/// Returns `true` if `path` is a `*.md` file inside an `entries/` directory.
///
/// Matches any depth: `eva/entries/step.md`, `shared/entries/note.md`, etc.
fn is_helix_entry(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("md") {
        return false;
    }
    path.components().any(|c| c.as_os_str() == "entries")
}

/// Returns `true` if `path` is a build tracking file inside `corso/builds/`.
///
/// Matches `active.yaml`, `portfolio.md`, and `roadmap.html` — the three
/// canonical build tracking artifacts.  These files are emitted as
/// [] instead of [] because they
/// represent structured build data rather than vault prose entries.
fn is_build_file(path: &Path) -> bool {
    // Must be under a `corso/builds/` directory at any depth.
    let components: Vec<_> = path.components().collect();
    let mut found_corso = false;
    let mut found_builds = false;
    for c in &components {
        if c.as_os_str() == "corso" {
            found_corso = true;
        }
        if found_corso && c.as_os_str() == "builds" {
            found_builds = true;
        }
    }
    if !found_builds {
        return false;
    }
    // Only match known build file extensions.
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "yaml" | "yml" | "md" | "html")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── is_helix_entry ────────────────────────────────────────────────────────

    #[test]
    fn accepts_entries_md_file() {
        let p = PathBuf::from("/helix/eva/entries/step.md");
        assert!(is_helix_entry(&p));
    }

    #[test]
    fn accepts_nested_entries_path() {
        let p = PathBuf::from("/helix/shared/conversations/entries/note.md");
        assert!(is_helix_entry(&p));
    }

    #[test]
    fn rejects_non_md_extension() {
        let p = PathBuf::from("/helix/eva/entries/step.txt");
        assert!(!is_helix_entry(&p));
    }

    #[test]
    fn rejects_md_outside_entries_dir() {
        let p = PathBuf::from("/helix/eva/identity.md");
        assert!(!is_helix_entry(&p));
    }

    #[test]
    fn rejects_no_extension() {
        let p = PathBuf::from("/helix/eva/entries/step");
        assert!(!is_helix_entry(&p));
    }

    // ── HelixEntrySummary serialisation ───────────────────────────────────────

    #[test]
    fn helix_entry_summary_serialises_relative_path_and_kind() {
        let entry = HelixEntrySummary {
            path: "eva/entries/day-1.md".to_owned(),
            event_kind: HelixEventKind::Created,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("eva/entries/day-1.md"), "{json}");
        assert!(json.contains("created"), "{json}");
    }

    #[test]
    fn helix_entry_summary_modified_kind_serialises() {
        let entry = HelixEntrySummary {
            path: "corso/entries/build.md".to_owned(),
            event_kind: HelixEventKind::Modified,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("modified"), "{json}");
    }

    // ── is_build_file ────────────────────────────────────────────────────────

    #[test]
    fn accepts_active_yaml() {
        let p = PathBuf::from("/helix/corso/builds/active.yaml");
        assert!(is_build_file(&p));
    }

    #[test]
    fn accepts_portfolio_md() {
        let p = PathBuf::from("/helix/corso/builds/portfolio.md");
        assert!(is_build_file(&p));
    }

    #[test]
    fn accepts_roadmap_html() {
        let p = PathBuf::from("/helix/corso/builds/roadmap.html");
        assert!(is_build_file(&p));
    }

    #[test]
    fn rejects_file_outside_builds() {
        let p = PathBuf::from("/helix/corso/entries/step.md");
        assert!(!is_build_file(&p));
    }

    #[test]
    fn rejects_yaml_outside_corso() {
        let p = PathBuf::from("/helix/eva/config.yaml");
        assert!(!is_build_file(&p));
    }

    #[test]
    fn rejects_non_build_extension() {
        let p = PathBuf::from("/helix/corso/builds/data.json");
        assert!(!is_build_file(&p));
    }
}
