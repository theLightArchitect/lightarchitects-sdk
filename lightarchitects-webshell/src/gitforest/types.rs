//! `GitForest` domain types — source-of-truth schema for the 4-level branch
//! hierarchy shared with the TypeScript frontend.
//!
//! JSON serialisation uses `snake_case` throughout so the TypeScript literal
//! union types align without any transformation layer.

use serde::{Deserialize, Serialize};

// ── Enumerations ──────────────────────────────────────────────────────────────

/// The structural role of a branch in the 4-level forest hierarchy.
///
/// | Level | Kind         | Example                      |
/// |-------|--------------|------------------------------|
/// | 0     | `main`       | `main`                       |
/// | 1     | `program`    | `feat/northstar-program-wave3`|
/// | 2     | `build`      | `feat/gitforest-live-ops`    |
/// | 3     | `wave_cluster` | `wave-3`                   |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchKind {
    /// Level 0 — the `main` trunk of the repository.
    Main,
    /// Level 1 — a program feature stub (e.g. `feat/northstar-program-wave3`).
    Program,
    /// Level 2 — a concrete build branch (e.g. `feat/gitforest-live-ops`).
    Build,
    /// Level 3 — a wave cluster branch grouping agent worktrees for one wave.
    WaveCluster,
}

/// Visual lifecycle state driving colour-saturation + opacity decay over time.
///
/// Merged branches are **never removed** — they transition through states so
/// the forest accumulates history as a visual memory layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchLifecycle {
    /// Branch is open with at least one active agent writing or gating.
    LiveActive,
    /// Branch is open but no agents are currently active.
    LiveIdle,
    /// Branch merged into its parent; opacity decays over 60 days, floor 0.20.
    Merged,
    /// Branch was closed without merging; rendered with sepia tint.
    Abandoned,
}

/// CI pipeline result for the most recent commit on this branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiStatus {
    /// All checks passed.
    Success,
    /// One or more checks failed.
    Failure,
    /// Checks are still running.
    Pending,
    /// No checks configured or checks were skipped.
    Neutral,
    /// CI status could not be determined.
    Unknown,
}

/// Whether a human-in-the-loop gate is blocking progress on this branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitlState {
    /// No HITL gate is active.
    None,
    /// An operator decision is required before the build can continue.
    Pending,
    /// The HITL gate was reviewed and cleared.
    Resolved,
}

/// Current activity state of a single agent worktree leaf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeState {
    /// Agent is actively writing code or generating output.
    Writing,
    /// Agent is waiting at a quality gate.
    Gate,
    /// Agent completed its task successfully.
    Done,
    /// Agent task failed; manual intervention may be required.
    Failed,
}

// ── Composite types ───────────────────────────────────────────────────────────

/// Overlay metadata attached to every [`BranchNode`].
///
/// Drives the colour-saturation + opacity decay model for persistent-merged
/// structures (see `BranchLifecycle` for the lifecycle table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchOverlayMeta {
    /// Current LASDLC phase label (e.g. `"phase-1-foundation"`); `None` for non-build branches.
    pub phase: Option<String>,
    /// Most recent quality-gate score in `0.0..=1.0`; `None` if no gate has run yet.
    pub gate_score: Option<f32>,
    /// How many calendar days ago this branch was created.
    pub age_days: u32,
    /// CI pipeline status for the most recent commit.
    pub ci_status: CiStatus,
    /// Whether a HITL gate is currently blocking this branch.
    pub hitl_state: HitlState,
    /// Internal squad model IDs credited to recent commits on this branch.
    pub model_attribution: Vec<String>,
    /// Visual lifecycle state governing opacity/saturation decay.
    pub lifecycle: BranchLifecycle,
    /// ISO-8601 timestamp; `None` while the branch is still open.
    pub merged_at: Option<String>,
    /// Branch name the merge targeted; `None` while open.
    pub merged_to: Option<String>,
    /// `clamp(1 - (now - merged_at) / 60d, 0.20, 1.0)`. Smooth 60-day decay;
    /// floor at 0.20 so fossil layers remain visible indefinitely.
    pub fade_level: f32,
}

/// Wave-level progress counter for build branches (`kind == Build`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgress {
    /// Number of LASDLC waves that have completed successfully.
    pub waves_done: u32,
    /// Total number of planned waves for this build.
    pub waves_total: u32,
}

/// A single agent's active worktree attached to a wave-cluster node.
///
/// `position_offset` is in `-1.0..=1.0` and controls radial fan placement
/// within the cluster so multiple agents spread out visually.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeAssignment {
    /// Unique agent instance key (e.g. `"eng-1"`).
    pub agent_key: String,
    /// Agent domain (e.g. `"engineer"`, `"quality"`).
    pub domain: String,
    /// Task ID from the `manifest.yaml` task table.
    pub task_id: String,
    /// Absolute filesystem path of the git worktree.
    pub worktree_path: String,
    /// Number of commits this agent has made in its worktree.
    pub commits: u32,
    /// Current activity state of this agent.
    pub state: WorktreeState,
    /// Radial offset within the wave-cluster fan: `-1.0` (left) to `1.0` (right).
    pub position_offset: f32,
}

/// A node in the 4-level `GitForest` branch hierarchy.
///
/// The tree is rooted at `kind == Main`; `children` contains the IDs of
/// direct child nodes. The full tree is returned as a flat ID-keyed map by
/// `GET /api/gitforest/topology`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchNode {
    /// Unique ID — typically the full git branch name (e.g. `feat/gitforest-live-ops`).
    pub id: String,
    /// Human-readable display name (may differ from `id` for wave clusters).
    pub name: String,
    /// Structural role in the 4-level hierarchy.
    pub kind: BranchKind,
    /// ID of the parent node; `None` for the root `main` node.
    pub parent_id: Option<String>,
    /// Tree depth: 0 = main, 1 = program, 2 = build, 3 = wave-cluster.
    pub depth: u8,
    /// Git commit SHA at which this branch diverged from its parent.
    pub fork_commit_sha: Option<String>,
    /// `0.0..=1.0` — position along the parent branch at which this node forks.
    pub fork_position: f32,
    /// IDs of direct children.
    pub children: Vec<String>,
    /// Overlay metadata driving colour-saturation, opacity, and lifecycle display.
    pub overlay: BranchOverlayMeta,
    /// Only present when `kind == Build`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_progress: Option<BuildProgress>,
    /// Only populated when `kind == WaveCluster`.
    #[serde(default)]
    pub worktrees: Vec<WorktreeAssignment>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn default_overlay() -> BranchOverlayMeta {
        BranchOverlayMeta {
            phase: None,
            gate_score: Some(0.95),
            age_days: 2,
            ci_status: CiStatus::Success,
            hitl_state: HitlState::None,
            model_attribution: vec!["sonnet".to_owned()],
            lifecycle: BranchLifecycle::LiveActive,
            merged_at: None,
            merged_to: None,
            fade_level: 1.0,
        }
    }

    #[test]
    fn branch_kind_serialises_to_snake_case() {
        let json = serde_json::to_string(&BranchKind::WaveCluster).unwrap();
        assert_eq!(json, r#""wave_cluster""#);
    }

    #[test]
    fn branch_lifecycle_serialises_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&BranchLifecycle::LiveActive).unwrap(),
            r#""live_active""#,
        );
        assert_eq!(
            serde_json::to_string(&BranchLifecycle::Merged).unwrap(),
            r#""merged""#,
        );
    }

    #[test]
    fn ci_status_serialises_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&CiStatus::Pending).unwrap(),
            r#""pending""#,
        );
    }

    #[test]
    fn hitl_state_serialises_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&HitlState::Pending).unwrap(),
            r#""pending""#,
        );
    }

    #[test]
    fn branch_node_round_trips() {
        let node = BranchNode {
            id: "feat/test-branch".to_owned(),
            name: "feat/test-branch".to_owned(),
            kind: BranchKind::Build,
            parent_id: Some("feat/northstar-program".to_owned()),
            depth: 2,
            fork_commit_sha: Some("abc123".to_owned()),
            fork_position: 0.75,
            children: vec!["wave-1".to_owned()],
            overlay: default_overlay(),
            build_progress: Some(BuildProgress {
                waves_done: 3,
                waves_total: 7,
            }),
            worktrees: vec![],
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: BranchNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, node.id);
        assert_eq!(back.depth, 2);
        assert_eq!(back.build_progress.unwrap().waves_done, 3);
    }

    #[test]
    fn worktree_assignment_state_serialises() {
        let wt = WorktreeAssignment {
            agent_key: "eng-1".to_owned(),
            domain: "engineer".to_owned(),
            task_id: "task-42".to_owned(),
            worktree_path: "~/lightarchitects/worktrees/foo".to_owned(),
            commits: 5,
            state: WorktreeState::Writing,
            position_offset: 0.3,
        };
        let json = serde_json::to_string(&wt).unwrap();
        assert!(json.contains(r#""writing""#));
    }
}
