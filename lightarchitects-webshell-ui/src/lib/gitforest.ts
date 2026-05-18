/**
 * GitForest domain types — TypeScript mirror of the Rust source-of-truth shapes
 * in `lightarchitects-webshell/src/gitforest/types.rs`.
 *
 * Serialisation uses `snake_case` throughout to match Rust `#[serde(rename_all = "snake_case")]`.
 * TypeScript literal unions must exactly match the serialised Rust variant names.
 */

// ── Enumerations ──────────────────────────────────────────────────────────────

/**
 * Structural role of a branch in the 4-level forest hierarchy.
 *
 * | Level | Kind           | Example                         |
 * |-------|----------------|---------------------------------|
 * | 0     | `main`         | `main`                          |
 * | 1     | `program`      | `feat/northstar-program-wave3`  |
 * | 2     | `build`        | `feat/gitforest-live-ops`       |
 * | 3     | `wave_cluster` | `wave-3`                        |
 */
export type BranchKind = 'main' | 'program' | 'build' | 'wave_cluster';

/**
 * Visual lifecycle state driving colour-saturation + opacity decay.
 *
 * Merged branches are **never removed** — they transition through states so
 * the forest accumulates history as a visual memory layer.
 */
export type BranchLifecycle = 'live_active' | 'live_idle' | 'merged' | 'abandoned';

/** CI pipeline result for the most recent commit on this branch. */
export type CiStatus = 'success' | 'failure' | 'pending' | 'neutral' | 'unknown';

/** Whether a human-in-the-loop gate is blocking progress. */
export type HitlState = 'none' | 'pending' | 'resolved';

/** Current activity state of a single agent worktree leaf. */
export type WorktreeState = 'writing' | 'gate' | 'done' | 'failed';

// ── Agent presence (polytope cluster) ────────────────────────────────────────

/**
 * Polytope kind used to encode agent-presence density.
 * Count → cluster mapping lives in `POLYTOPE_CLUSTER_MAP`.
 */
export type PolytopeKind =
  | 'pentachoron'
  | 'tesseract'
  | 'hexadecachoron'
  | 'icositetrachoron'
  | 'rectified5cell'
  | 'duoprism55';

/** Maps active-agent count to the polytope cluster to render. */
export const POLYTOPE_CLUSTER_MAP: Record<number, PolytopeKind[]> = {
  0: [],
  1: ['pentachoron'],
  2: ['pentachoron', 'tesseract'],
  3: ['pentachoron', 'tesseract', 'hexadecachoron'],
  4: ['pentachoron', 'tesseract', 'hexadecachoron', 'icositetrachoron'],
};

/**
 * Returns the polytope cluster for a given active-agent count.
 * Counts ≥ 5 receive the count-4 cluster (max visual variety).
 */
export function polytopeClusterFor(count: number): PolytopeKind[] {
  if (count <= 0) return [];
  const capped = Math.min(count, 4);
  return POLYTOPE_CLUSTER_MAP[capped] ?? [];
}

/** Derived agent-presence data for a branch node, used by the renderer. */
export interface AgentPresence {
  /** Recursive sum across all descendant worktree leaves in active states. */
  active_count: number;
  /** Polytope cluster kinds derived from `active_count`. */
  polytope_cluster: PolytopeKind[];
  /** Canvas anchor point for the polytope cluster (branch tip or wave centroid). */
  anchor: { x: number; y: number };
}

// ── Composite types ───────────────────────────────────────────────────────────

/** Overlay metadata attached to every `BranchNode`. */
export interface BranchOverlayMeta {
  phase: string | null;
  gate_score: number | null;
  age_days: number;
  ci_status: CiStatus;
  hitl_state: HitlState;
  /** Internal squad model IDs credited to recent commits. */
  model_attribution: string[];
  lifecycle: BranchLifecycle;
  /** ISO-8601 timestamp; `null` while the branch is still open. */
  merged_at: string | null;
  /** Branch name the merge targeted; `null` while open. */
  merged_to: string | null;
  /**
   * `clamp(1 - (now - merged_at) / 60d, 0.20, 1.0)`.
   * Smooth 60-day decay; floor at 0.20 so fossil layers remain visible.
   */
  fade_level: number;
}

/** Wave-level progress counter — only present on `kind === 'build'` nodes. */
export interface BuildProgress {
  waves_done: number;
  waves_total: number;
}

/**
 * A single agent's active worktree leaf within a wave-cluster node.
 *
 * `position_offset` is in `-1..1` and controls radial fan placement within
 * the cluster so multiple agents spread out visually.
 */
export interface WorktreeAssignment {
  agent_key: string;
  domain: string;
  task_id: string;
  worktree_path: string;
  commits: number;
  state: WorktreeState;
  /** Radial offset within the wave-cluster fan: `-1` (left) to `1` (right). */
  position_offset: number;
}

/**
 * A node in the 4-level GitForest branch hierarchy.
 *
 * The full tree is returned as a flat record keyed by `id` from
 * `GET /api/gitforest/topology`. `children` contains the IDs of direct
 * child nodes; reconstruct the tree by following `parent_id` links.
 */
export interface BranchNode {
  /** Unique ID — typically the full git branch name. */
  id: string;
  name: string;
  kind: BranchKind;
  parent_id: string | null;
  /** Tree depth: 0 = main, 1 = program, 2 = build, 3 = wave-cluster. */
  depth: 0 | 1 | 2 | 3;
  fork_commit_sha: string | null;
  /** `0..1` — position along the parent branch at which this node forks. */
  fork_position: number;
  /** IDs of direct children. */
  children: string[];
  overlay: BranchOverlayMeta;
  /** Only present when `kind === 'build'`. */
  build_progress: BuildProgress | null;
  /** Only populated when `kind === 'wave_cluster'`. */
  worktrees: WorktreeAssignment[];
}

// ── Forest topology response ───────────────────────────────────────────────────

/** Response shape for `GET /api/gitforest/topology`. */
export interface GitForestTopology {
  /** Repository name — matches the root `BranchNode.id`. */
  repo: string;
  /** Flat map of all nodes keyed by `BranchNode.id`. */
  nodes: Record<string, BranchNode>;
  /** ID of the root `main` node. */
  root_id: string;
  /** ISO-8601 timestamp of the last topology change. */
  fetched_at: string;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Computes the `fade_level` for a merged branch.
 * Returns `1.0` for live branches. Floor is `0.20`.
 */
export function computeFadeLevel(mergedAt: string | null): number {
  if (!mergedAt) return 1.0;
  const ageMs = Date.now() - new Date(mergedAt).getTime();
  const ageDays = ageMs / 86_400_000;
  return Math.max(0.2, Math.min(1.0, 1 - ageDays / 60));
}

/**
 * Recursively counts the active worktree leaves under a node (inclusive).
 * Active = state is `'writing'` or `'gate'`.
 */
export function countActiveWorktrees(
  nodeId: string,
  nodes: Record<string, BranchNode>,
): number {
  const node = nodes[nodeId];
  if (!node) return 0;
  const ownActive = node.worktrees.filter(
    wt => wt.state === 'writing' || wt.state === 'gate',
  ).length;
  const childActive = node.children.reduce(
    (sum, cid) => sum + countActiveWorktrees(cid, nodes),
    0,
  );
  return ownActive + childActive;
}

/**
 * Build a `GitForestTopology` from a single `BranchNode` root received via SSE.
 *
 * Phase 2 scaffold: only the root node is in `nodes`; child nodes are referenced
 * by ID but not present. Phase 5 replaces this with the full REST fetch from
 * `GET /api/gitforest/topology` which populates all descendants.
 */
export function reconstructTopology(repo: string, root: BranchNode): GitForestTopology {
  return {
    repo,
    root_id: root.id,
    nodes: { [root.id]: root },
    fetched_at: new Date().toISOString(),
  };
}

/**
 * Fetch the full branch topology for a repo from the webshell REST API.
 *
 * Phase 4 provides the backing route `GET /api/gitforest/topology?repo=<name>`.
 * Until then, this returns null and callers degrade gracefully.
 */
export async function fetchTopology(repo: string): Promise<GitForestTopology | null> {
  try {
    const res = await fetch(`/api/gitforest/topology?repo=${encodeURIComponent(repo)}`);
    if (!res.ok) return null;
    return await res.json() as GitForestTopology;
  } catch {
    return null;
  }
}
