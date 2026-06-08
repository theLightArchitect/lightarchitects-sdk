//! Contradiction graph analysis — cycle detection and depth measurement.
//!
//! When a `Confidence` event arrives with a non-empty `contradicts` list, this
//! module determines whether the resulting contradiction graph has a cycle or
//! has reached `MAX_CONTRADICTION_DEPTH`. Either condition triggers a
//! `PendingResolution` to be synthesized.

use crate::types::{CanvasState, ConfidenceRecord, PendingResolution};
use std::collections::{HashMap, HashSet};

/// Maximum contradiction chain depth before a resolution is auto-synthesized.
pub const MAX_CONTRADICTION_DEPTH: u32 = 3;

/// Check for a cycle or excessive depth in the contradiction graph starting
/// from `target_id` after adding an edge to each entry in `contradicts`.
///
/// Returns `(cycle_detected, max_depth_reached)`.
pub fn detect_cycle_or_depth(
    records: &[ConfidenceRecord],
    target_id: &str,
    contradicts: &[String],
) -> (bool, u32) {
    // Build adjacency list from existing records + the new edges.
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for rec in records {
        for c in &rec.contradicts {
            adj.entry(rec.target_id.as_str())
                .or_default()
                .push(c.as_str());
        }
    }
    for c in contradicts {
        adj.entry(target_id).or_default().push(c.as_str());
    }

    let cycle = has_cycle(&adj);
    let depth = max_depth_from(&adj, target_id);
    (cycle, depth)
}

/// Synthesize a resolution: winner = highest-confidence contradicting target.
pub fn synthesize_resolution(
    state: &CanvasState,
    target_id: &str,
    contradicts: &[String],
    depth_reached: u32,
    cycle_yielded: bool,
) -> PendingResolution {
    let winner = pick_winner(state, target_id, contradicts);
    let all_targets: Vec<String> = std::iter::once(target_id.to_owned())
        .chain(contradicts.iter().cloned())
        .collect();
    let loser_target_ids = all_targets.into_iter().filter(|t| t != &winner).collect();

    PendingResolution {
        winner_target_id: winner,
        loser_target_ids,
        depth_reached,
        cycle_yielded,
        synthesized_at_seq: state.snapshot_seq,
    }
}

/// Apply a confirmed resolution: remove loser confidence records and clear
/// matching pending_resolutions.
pub fn apply_resolution(
    state: &mut CanvasState,
    winner_target_id: &str,
    loser_target_ids: &[String],
) {
    state
        .confidence_records
        .retain(|r| !loser_target_ids.contains(&r.target_id));
    state.pending_resolutions.retain(|p| {
        p.winner_target_id != winner_target_id && !loser_target_ids.contains(&p.winner_target_id)
    });
}

/// DFS cycle detection on an adjacency list.
fn has_cycle(adj: &HashMap<&str, Vec<&str>>) -> bool {
    let mut visited: HashSet<&str> = HashSet::new();
    let mut rec_stack: HashSet<&str> = HashSet::new();
    for &node in adj.keys() {
        if dfs_cycle(adj, node, &mut visited, &mut rec_stack) {
            return true;
        }
    }
    false
}

fn dfs_cycle<'a>(
    adj: &HashMap<&'a str, Vec<&'a str>>,
    node: &'a str,
    visited: &mut HashSet<&'a str>,
    rec_stack: &mut HashSet<&'a str>,
) -> bool {
    if rec_stack.contains(node) {
        return true;
    }
    if visited.contains(node) {
        return false;
    }
    visited.insert(node);
    rec_stack.insert(node);
    if let Some(neighbors) = adj.get(node) {
        for &neighbor in neighbors {
            if dfs_cycle(adj, neighbor, visited, rec_stack) {
                rec_stack.remove(node);
                return true;
            }
        }
    }
    rec_stack.remove(node);
    false
}

/// BFS maximum depth from `start` in the adjacency list.
fn max_depth_from(adj: &HashMap<&str, Vec<&str>>, start: &str) -> u32 {
    let mut max = 0u32;
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((start, 0u32));
    let mut seen: HashSet<&str> = HashSet::new();
    seen.insert(start);
    while let Some((node, depth)) = queue.pop_front() {
        if depth > max {
            max = depth;
        }
        if let Some(neighbors) = adj.get(node) {
            for &neighbor in neighbors {
                if !seen.contains(neighbor) {
                    seen.insert(neighbor);
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }
    }
    max
}

/// Pick the winner: the target with the highest confidence value.
fn pick_winner(state: &CanvasState, target_id: &str, contradicts: &[String]) -> String {
    let all: Vec<&str> = std::iter::once(target_id)
        .chain(contradicts.iter().map(String::as_str))
        .collect();
    let winner = all
        .iter()
        .max_by(|&&a, &&b| {
            let va = confidence_for(state, a);
            let vb = confidence_for(state, b);
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
        .unwrap_or(target_id);
    winner.to_owned()
}

/// Look up the latest confidence value for a target, defaulting to 0.0.
fn confidence_for(state: &CanvasState, target_id: &str) -> f64 {
    state
        .confidence_records
        .iter()
        .filter(|r| r.target_id == target_id)
        .map(|r| r.value)
        .next_back()
        .unwrap_or(0.0)
}
