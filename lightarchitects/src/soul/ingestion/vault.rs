//! Vault directory ingestion — walk a directory and stream [`StorageEntry`] values.
//!
//! [`load_directory`] returns an async `Stream` of parsed entries; one entry per
//! `*.md` file found under `dir`. [`ingest_directory`] is a convenience wrapper
//! that collects the stream and writes all entries to a [`StorageBackend`].
//!
//! # Path Safety
//!
//! Paths containing `..` components are rejected with a warning and skipped.
//! This prevents path-traversal attacks when the caller provides user-supplied
//! directory paths.
//!
//! # Capacity Bound
//!
//! [`ingest_directory`] processes at most [`MAX_ENTRIES`] entries per call and
//! emits a warning if the vault is larger. This prevents unbounded memory use
//! when ingesting large vaults.

use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt as _, stream};
use tracing::{instrument, warn};

use crate::soul::storage::{StorageBackend, StorageEntry, StorageError};

use super::markdown::from_markdown;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of entries loaded by [`ingest_directory`] in a single call.
pub const MAX_ENTRIES: usize = 10_000;

// ============================================================================
// load_directory
// ============================================================================

/// Walk `dir` recursively, yielding a [`StorageEntry`] for each `*.md` file.
///
/// Files are discovered with a synchronous recursive descent via `std::fs`.
/// Paths containing `..` components are rejected and logged as warnings.
///
/// The returned `Stream` yields items lazily — each `.md` file is read and
/// parsed as the stream is consumed.
///
/// # Errors
///
/// Individual file parse failures surface as `Err` items in the stream.
/// Directory read failures are logged as warnings and the affected directory
/// is skipped (not fatal).
pub fn load_directory(dir: &Path) -> impl Stream<Item = Result<StorageEntry, StorageError>> + '_ {
    let paths = collect_markdown_paths(dir);
    stream::iter(paths.into_iter().map(move |path| {
        let path_str = path.to_string_lossy().into_owned();
        let content = std::fs::read_to_string(&path)
            .map_err(|e| StorageError::Io(format!("read {}: {e}", path.display())))?;
        from_markdown(&path_str, &content)
    }))
}

/// Recursively collect all `*.md` file paths under `dir`.
///
/// Skips paths containing `..` components (path traversal guard).
/// Skips unreadable directories with a warning.
fn collect_markdown_paths(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        // Reject any path with a ParentDir component.
        if has_parent_dir_component(&current) {
            warn!(path = %current.display(), "skipping path with '..' component");
            continue;
        }

        let read_dir = match std::fs::read_dir(&current) {
            Ok(rd) => rd,
            Err(e) => {
                warn!(path = %current.display(), error = %e, "skipping unreadable directory");
                continue;
            }
        };

        for entry_result in read_dir {
            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    warn!(error = %e, "skipping unreadable entry");
                    continue;
                }
            };

            let entry_path = entry.path();

            if has_parent_dir_component(&entry_path) {
                warn!(path = %entry_path.display(), "skipping path with '..' component");
                continue;
            }

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    warn!(path = %entry_path.display(), error = %e, "cannot stat entry");
                    continue;
                }
            };

            if file_type.is_dir() {
                stack.push(entry_path);
            } else if file_type.is_file()
                && entry_path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                result.push(entry_path);
            }
        }
    }

    result
}

/// Returns `true` if any component of `path` is a `ParentDir` (`..`).
fn has_parent_dir_component(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

// ============================================================================
// ingest_directory + ingest_directory_with_embedding
// ============================================================================

/// Parallel embedding concurrency — number of batch calls in-flight simultaneously.
const EMBED_CONCURRENCY: usize = 4;

/// Maximum entries per embedding batch call.
///
/// Capped independently from the provider's `max_batch_size()` to bound
/// memory use per batch. Effective batch size is `min(EMBED_BATCH_SIZE, provider.max_batch_size())`.
const EMBED_BATCH_SIZE: usize = 32;

/// Walk `dir`, parse all `*.md` files, and write them to `backend`.
///
/// Processes at most [`MAX_ENTRIES`] entries. If the vault contains more,
/// a warning is emitted and the remaining entries are skipped.
///
/// Returns the number of entries written.
///
/// # Errors
///
/// Returns [`StorageError`] on file read failure or backend write failure.
#[instrument(skip(backend))]
pub async fn ingest_directory<B: StorageBackend>(
    dir: &Path,
    backend: &B,
) -> Result<usize, StorageError> {
    let stream = load_directory(dir);
    tokio::pin!(stream);

    let mut entries: Vec<StorageEntry> = Vec::new();
    let mut truncated = false;

    while let Some(result) = stream.next().await {
        let entry = result?;
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(entry);
    }

    if truncated {
        warn!(
            limit = MAX_ENTRIES,
            "ingest_directory: vault exceeds limit; remaining entries skipped"
        );
    }

    let count = backend.write_entries_batch(&entries).await?;
    tracing::info!(count, dir = %dir.display(), "ingest_directory complete");
    Ok(count)
}

/// Walk `dir`, write all `*.md` files to `backend`, then embed each entry's
/// content using `embedder` with bounded batch parallelism.
///
/// Entries are written first (via [`StorageBackend::write_entries_batch`]), then
/// content is embedded in batches of up to
/// `min(EMBED_BATCH_SIZE, embedder.max_batch_size())` entries, with up to
/// [`EMBED_CONCURRENCY`] batches dispatched concurrently via
/// [`StreamExt::buffer_unordered`].
///
/// Embedding failures are **non-fatal** — a warning is emitted per failed batch
/// but the return value reflects successfully written entries, not embeddings.
/// This ensures ingestion never rolls back due to a temporarily unavailable
/// embedding provider.
///
/// # Errors
///
/// Returns [`StorageError`] on file read failure or backend write failure.
/// Embedding failures do not propagate as errors — see the non-fatal note above.
#[instrument(skip(backend, embedder))]
pub async fn ingest_directory_with_embedding<B: StorageBackend>(
    dir: &Path,
    backend: &B,
    embedder: &dyn crate::soul::embedding::EmbeddingProvider,
) -> Result<usize, StorageError> {
    let stream = load_directory(dir);
    tokio::pin!(stream);

    let mut entries: Vec<StorageEntry> = Vec::new();
    let mut truncated = false;

    while let Some(result) = stream.next().await {
        let entry = result?;
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(entry);
    }

    if truncated {
        warn!(
            limit = MAX_ENTRIES,
            "ingest_directory_with_embedding: vault exceeds limit; remaining entries skipped"
        );
    }

    let count = backend.write_entries_batch(&entries).await?;

    // Dispatch embedding in bounded-parallel batches.
    // min() against max_batch_size() respects provider limits (e.g. Ollama default 512).
    let batch_size = embedder.max_batch_size().clamp(1, EMBED_BATCH_SIZE);

    let embed_fail_count: usize = stream::iter(entries.chunks(batch_size))
        .map(|chunk| async move {
            let texts: Vec<&str> = chunk.iter().map(|e| e.content.as_str()).collect();
            match embedder.embed(&texts).await {
                Err(e) => {
                    warn!(
                        batch_size = chunk.len(),
                        error = %e,
                        "batch embed call failed (non-fatal)"
                    );
                    chunk.len()
                }
                Ok(vecs) => {
                    let mut fails = 0usize;
                    for (entry, vec) in chunk.iter().zip(vecs.iter()) {
                        if let Err(e) = backend
                            .write_embedding(&entry.id, embedder.name(), vec)
                            .await
                        {
                            warn!(
                                entry_id = %entry.id,
                                error = %e,
                                "write_embedding failed (non-fatal)"
                            );
                            fails = fails.saturating_add(1);
                        }
                    }
                    fails
                }
            }
        })
        .buffer_unordered(EMBED_CONCURRENCY)
        .fold(0usize, |acc, n| async move { acc.saturating_add(n) })
        .await;

    if embed_fail_count > 0 {
        warn!(
            embed_fail_count,
            "ingest_directory_with_embedding: some embeddings failed (non-fatal)"
        );
    }

    tracing::info!(count, dir = %dir.display(), "ingest_directory_with_embedding complete");
    Ok(count)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).expect("write test file");
    }

    #[tokio::test]
    async fn test_load_directory_finds_md_files() {
        let tmp = TempDir::new().expect("tempdir");
        write_file(
            tmp.path(),
            "entry1.md",
            "---\ntitle: First\n---\nContent one.",
        );
        write_file(
            tmp.path(),
            "entry2.md",
            "---\ntitle: Second\n---\nContent two.",
        );
        write_file(tmp.path(), "ignored.txt", "not markdown");

        let stream = load_directory(tmp.path());
        tokio::pin!(stream);

        let mut entries: Vec<StorageEntry> = Vec::new();
        while let Some(result) = stream.next().await {
            entries.push(result.expect("entry should parse"));
        }

        assert_eq!(entries.len(), 2, "should find exactly 2 .md files");
    }

    #[tokio::test]
    async fn test_load_directory_empty_dir_yields_nothing() {
        let tmp = TempDir::new().expect("tempdir");
        let stream = load_directory(tmp.path());
        tokio::pin!(stream);

        let mut count = 0usize;
        while let Some(_result) = stream.next().await {
            count = count.saturating_add(1);
        }
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_load_directory_recurses_into_subdirectories() {
        let tmp = TempDir::new().expect("tempdir");
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).expect("create subdir");

        write_file(tmp.path(), "root.md", "Root entry.");
        write_file(&sub, "nested.md", "Nested entry.");

        let stream = load_directory(tmp.path());
        tokio::pin!(stream);

        let mut count = 0usize;
        while let Some(result) = stream.next().await {
            result.expect("entry parse");
            count = count.saturating_add(1);
        }
        assert_eq!(count, 2, "should find both root and nested entries");
    }

    #[test]
    fn test_has_parent_dir_component_detects_dotdot() {
        assert!(has_parent_dir_component(Path::new("some/../path")));
        assert!(!has_parent_dir_component(Path::new("some/safe/path")));
    }
}
