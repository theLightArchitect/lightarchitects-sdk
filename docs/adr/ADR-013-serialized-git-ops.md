# ADR-013: Serialized git ops — Arc<Mutex<()>> for ref-mutating operations + git2 carve-outs

**Status**: Accepted
**Date**: 2026-05-18
**Authors**: Kevin (architect), Claude (engineer)
**Phase prerequisite**: Phase 3 (merge_agent.rs)
**Related**: ADR-009 (SDK), ADR-010 (worker spawn), `memory://reference_git_library_landscape_2026`

---

## Context

lightsquad's `MergeAgent` serializes task-branch merges to `feat/ironclaw-spine`. With 7
concurrent worker slots, multiple workers can finish simultaneously and compete to merge.
Concurrent git operations that mutate refs (merge, commit, rebase, push) produce race
conditions even on separate branches if the index or reflog is shared.

Three library options were evaluated (per R-Task #17 research, 2026-05-18):

1. **`gix` (gitoxide)** — pure-Rust, async-native. No `add_worktree` primitive in the
   current version (confirmed via Context7 crate-status lookup 2026-05-18). Missing the
   primary operation needed.

2. **`git2` (libgit2 bindings)** — mature, synchronous. `Repository::worktree_add` exists
   but lacks `--force` and `--detach` flags needed for ironclaw's topology.

3. **`Command::new("git")` subprocess** — shells out to the system `git` binary.
   Full flag support. Honors git hooks (Cookbook §64.5). Only option that supports
   `git worktree add --force --detach` + full `git merge --no-ff` semantics.

## Decision

**100% `Command::new("git")` for ref-mutating operations; `git2` carve-outs for cheap reads only.**

Ref-mutating operations (merge, worktree add, commit, push, reset) use `Command::new("git")`.
Cheap read operations (current HEAD SHA, branch name) may use `git2` for speed.

```rust
// merge_agent.rs
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MergeAgent {
    git_lock: Arc<Mutex<()>>,  // serializes ALL ref-mutating git ops globally
    repo_root: PathBuf,
}

impl MergeAgent {
    pub async fn merge_task_to_feat(&self, task_branch: &str) -> Result<Oid, MergeError> {
        // Acquire global git lock — only one merge at a time
        let _guard = self.git_lock.lock().await;

        // Jittered exponential retry for transient lock contention
        let backoff = ExponentialBackoff::new(
            Duration::from_millis(100),
            2.0,
            Duration::from_secs(30),
            Some(0.2),  // 20% jitter
        );

        retry_with_backoff(backoff, || {
            self.run_git(&["merge", "--no-ff", task_branch])
        }).await
    }

    fn run_git(&self, args: &[&str]) -> Result<Output, MergeError> {
        Command::new("git")
            .current_dir(&self.repo_root)
            .args(args)
            .output()
            .map_err(MergeError::IoError)
    }
}
```

## Consequences

- **`Arc<Mutex<()>>` scope is ref-mutating-only** — non-mutating operations (read, status, log)
  do NOT acquire the lock. Lock is not a full git serialization — it's a targeted guard.
- **Jittered exponential retry** prevents thundering-herd when 7 workers finish simultaneously.
  Base 100ms, factor 2.0, max 30s, 20% jitter. (Cookbook §64.2)
- **Git hooks are honored** — `Command::new("git")` triggers pre-commit / post-merge hooks
  normally. `git2` does not invoke hooks. This is the primary reason for the Command preference.
- **`gix` deferred** — no `add_worktree` primitive means `gix` can't replace git for the
  worktree lifecycle operations ironclaw needs. Revisit when gix adds this (open issue).
- **`git2` carve-outs** — `Repository::head()` + `Repository::find_branch()` for cheap reads
  where fork/exec overhead is wasteful (e.g., every-tick heartbeat SHA reads in supervisor).

## Alternatives rejected

- **`gix` for all ops**: Missing `add_worktree` primitive (confirmed, not speculative).
  Cannot be primary library. Rejected until the primitive lands upstream.
- **`git2` for all ops**: Lacks `--force`/`--detach` flags for `worktree_add`. Does not
  invoke hooks. Rejected as primary library.
- **Unserialized concurrent merges**: Race condition on the reflog under concurrent workers.
  Confirmed failure mode. Rejected.
