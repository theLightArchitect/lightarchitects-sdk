//! Git worktree lifecycle management — shared surface with gitforest.
//!
//! Phase 3 implementation:
//! - `WorktreeManager { ops_mutex: Arc<Mutex<()>>, repo_root: PathBuf }`
//! - `create(&self, branch, path) -> Result<WorktreeHandle, WorktreeError>`
//! - `remove(&self, path) -> Result<(), WorktreeError>` (with cleanup protocol)
//! - `list(&self) -> Result<Vec<Worktree>, WorktreeError>` — bypasses mutex (pure read)
//!
//! `list()` is a shared surface: when ironclaw ships, gitforest's
//! `GET /api/git/worktrees/{repo}` migrates to call this method (§2.10c canon amendment).
//! All ref-mutating ops (create/remove) are serialised behind `ops_mutex`.
