//! Investigation Task Tree (ITT) strategy — SDK port of QUANTUM agentic/itt.rs.
//!
//! Breadth-first hypothesis exploration: each step expands one unexplored node,
//! collects evidence, and verifies the hypothesis. Low-confidence branches are pruned.
//!
//! Source: QUANTUM internal, based on LATS (Language Agent Tree Search, Zhou et al. 2023)

use std::collections::HashSet;

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Types (ported from QUANTUM) ───────────────────────────────────────────────

/// Node identifier (case-scoped, unique per tree).
pub type NodeId = String;

/// Investigation phase that created or owns a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QPhase {
    /// Phase 1 — passive baseline scan.
    Scan,
    /// Phase 2 — active evidence sweep.
    Sweep,
    /// Phase 3 — trace pattern matching.
    Trace,
    /// Phase 4 — targeted probe.
    Probe,
    /// Phase 5 — hypothesis generation.
    Theorize,
    /// Phase 6 — hypothesis verification.
    Verify,
    /// Phase 7 — close investigation.
    Close,
}

impl std::fmt::Display for QPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Scan => "SCAN",
            Self::Sweep => "SWEEP",
            Self::Trace => "TRACE",
            Self::Probe => "PROBE",
            Self::Theorize => "THEORIZE",
            Self::Verify => "VERIFY",
            Self::Close => "CLOSE",
        })
    }
}

/// A pointer to a piece of evidence collected during the investigation.
#[derive(Debug, Clone)]
pub struct EvidenceRef {
    /// Unique evidence identifier.
    pub id: String,
    /// File path or resource the evidence was extracted from.
    pub path: String,
    /// Human-readable description.
    pub description: String,
    /// Phase that collected this evidence.
    pub collected_by: QPhase,
}

/// Result of verifying a hypothesis node.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether the hypothesis was confirmed.
    pub confirmed: bool,
    /// Evidence supporting or refuting the hypothesis.
    pub evidence_ids: Vec<String>,
    /// Confidence score after verification (0.0–1.0).
    pub confidence: f64,
    /// Human-readable conclusion.
    pub conclusion: String,
}

/// A single node in the [`InvestigationTaskTree`].
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Unique node identifier.
    pub id: NodeId,
    /// Phase that created this node.
    pub phase: QPhase,
    /// Hypothesis text.
    pub hypothesis: String,
    /// Confidence score (0.0–1.0).
    pub confidence: f64,
    /// References to collected evidence.
    pub evidence: Vec<EvidenceRef>,
    /// Verification result (set during the Verify phase).
    pub verification: Option<VerificationResult>,
    /// Child hypothesis nodes.
    pub children: Vec<TreeNode>,
    /// Whether this branch has been pruned (soft delete — recoverable).
    pub pruned: bool,
}

/// The complete investigation task tree.
#[derive(Debug, Clone)]
pub struct InvestigationTaskTree {
    /// Case identifier.
    pub case_id: String,
    /// Root node (the case statement).
    pub root: TreeNode,
    /// Currently active branch being explored.
    pub active_branch: Option<NodeId>,
    /// Nodes that have been fully explored.
    pub explored: HashSet<NodeId>,
    /// Nodes awaiting exploration (breadth-first order).
    pub unexplored: Vec<NodeId>,
    node_counter: u32,
}

impl InvestigationTaskTree {
    /// Create a new ITT for a case.
    #[must_use]
    pub fn new(case_id: impl Into<String>, initial_hypothesis: impl Into<String>) -> Self {
        let case_id = case_id.into();
        let root_id = format!("{case_id}-root");
        let root = TreeNode {
            id: root_id.clone(),
            phase: QPhase::Scan,
            hypothesis: initial_hypothesis.into(),
            confidence: 0.0,
            evidence: Vec::new(),
            verification: None,
            children: Vec::new(),
            pruned: false,
        };
        let mut explored = HashSet::new();
        explored.insert(root_id.clone());
        Self {
            case_id,
            root,
            active_branch: Some(root_id),
            explored,
            unexplored: Vec::new(),
            node_counter: 1,
        }
    }

    /// Add a child hypothesis to a parent node.
    ///
    /// Returns the new node ID, or `None` if the parent was not found.
    pub fn add_hypothesis(
        &mut self,
        parent_id: &str,
        hypothesis: impl Into<String>,
        phase: QPhase,
        confidence: f64,
    ) -> Option<NodeId> {
        self.node_counter += 1;
        let new_id = format!("{}-h{}", self.case_id, self.node_counter);
        let new_node = TreeNode {
            id: new_id.clone(),
            phase,
            hypothesis: hypothesis.into(),
            confidence,
            evidence: Vec::new(),
            verification: None,
            children: Vec::new(),
            pruned: false,
        };
        if Self::add_child_to_node(&mut self.root, parent_id, new_node) {
            self.unexplored.push(new_id.clone());
            Some(new_id)
        } else {
            None
        }
    }

    /// Attach evidence to a node.
    pub fn attach_evidence(&mut self, node_id: &str, evidence: EvidenceRef) -> bool {
        Self::modify_node(&mut self.root, node_id, |n| n.evidence.push(evidence))
    }

    /// Set the verification result on a node.
    pub fn set_verification(&mut self, node_id: &str, result: VerificationResult) -> bool {
        Self::modify_node(&mut self.root, node_id, |n| n.verification = Some(result))
    }

    /// Update the confidence score for a node.
    pub fn update_confidence(&mut self, node_id: &str, confidence: f64) -> bool {
        Self::modify_node(&mut self.root, node_id, |n| n.confidence = confidence)
    }

    /// Soft-delete a branch (recoverable via [`recover`]).
    ///
    /// [`recover`]: Self::recover
    pub fn prune(&mut self, node_id: &str) -> bool {
        Self::modify_node(&mut self.root, node_id, |n| n.pruned = true)
    }

    /// Recover a previously pruned branch.
    pub fn recover(&mut self, node_id: &str) -> bool {
        Self::modify_node(&mut self.root, node_id, |n| n.pruned = false)
    }

    /// Mark a node as the active branch being explored.
    pub fn set_active(&mut self, node_id: &str) {
        self.active_branch = Some(node_id.to_string());
        self.explored.insert(node_id.to_string());
        self.unexplored.retain(|id| id != node_id);
    }

    /// Return the next unexplored node (breadth-first).
    #[must_use]
    pub fn next_unexplored(&self) -> Option<&NodeId> {
        self.unexplored.first()
    }

    /// Find a node by ID.
    #[must_use]
    pub fn find_node(&self, node_id: &str) -> Option<&TreeNode> {
        Self::find_in_tree(&self.root, node_id)
    }

    /// Return all leaf nodes (no children, not pruned).
    #[must_use]
    pub fn leaves(&self) -> Vec<&TreeNode> {
        let mut leaves = Vec::new();
        Self::collect_leaves(&self.root, &mut leaves);
        leaves
    }

    /// Return all hypothesis nodes ranked by confidence (highest first).
    #[must_use]
    pub fn ranked_hypotheses(&self) -> Vec<&TreeNode> {
        let mut nodes = Vec::new();
        Self::collect_hypotheses(&self.root, &mut nodes);
        nodes.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        nodes
    }

    /// Return the top hypothesis (highest confidence, not pruned).
    #[must_use]
    pub fn top_hypothesis(&self) -> Option<&TreeNode> {
        self.ranked_hypotheses().into_iter().next()
    }

    /// Linear confidence heuristic: `evidence_weight + pattern_bonus + historical`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn linear_score(evidence_count: usize, pattern_match: bool, historical_hits: usize) -> f64 {
        let evidence_weight = (evidence_count as f64 * 0.15).min(0.6);
        let pattern_bonus = if pattern_match { 0.25 } else { 0.0 };
        let historical = (historical_hits as f64 * 0.1).min(0.3);
        (evidence_weight + pattern_bonus + historical).min(1.0)
    }

    // --- Internal helpers ---

    fn add_child_to_node(node: &mut TreeNode, parent_id: &str, child: TreeNode) -> bool {
        if node.id == parent_id {
            node.children.push(child);
            return true;
        }
        for child_node in &mut node.children {
            if Self::add_child_to_node(child_node, parent_id, child.clone()) {
                return true;
            }
        }
        false
    }

    fn modify_node<F: FnOnce(&mut TreeNode)>(node: &mut TreeNode, target_id: &str, f: F) -> bool {
        if node.id == target_id {
            f(node);
            return true;
        }
        let idx = node
            .children
            .iter()
            .position(|c| Self::find_in_tree(c, target_id).is_some());
        if let Some(i) = idx {
            return Self::modify_node(&mut node.children[i], target_id, f);
        }
        false
    }

    fn find_in_tree<'a>(node: &'a TreeNode, target_id: &str) -> Option<&'a TreeNode> {
        if node.id == target_id {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = Self::find_in_tree(child, target_id) {
                return Some(found);
            }
        }
        None
    }

    fn collect_leaves<'a>(node: &'a TreeNode, out: &mut Vec<&'a TreeNode>) {
        if node.pruned {
            return;
        }
        if node.children.is_empty() || node.children.iter().all(|c| c.pruned) {
            out.push(node);
        } else {
            for child in &node.children {
                Self::collect_leaves(child, out);
            }
        }
    }

    fn collect_hypotheses<'a>(node: &'a TreeNode, out: &mut Vec<&'a TreeNode>) {
        if !node.pruned && node.phase != QPhase::Scan {
            out.push(node);
        }
        for child in &node.children {
            Self::collect_hypotheses(child, out);
        }
    }
}

// ── Executor ──────────────────────────────────────────────────────────────────

/// Provider-agnostic executor for one ITT exploration step.
#[async_trait]
pub trait IttExecutor: Send + Sync + 'static {
    /// Expand a node by generating child hypotheses.
    ///
    /// Returns `(hypothesis_text, confidence)` pairs.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn expand(
        &self,
        node_id: &str,
        hypothesis: &str,
        ctx: &StepContext,
    ) -> Result<Vec<(String, f64)>, LoopError>;

    /// Collect evidence for a hypothesis node.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn collect_evidence(
        &self,
        node_id: &str,
        hypothesis: &str,
        ctx: &StepContext,
    ) -> Result<Vec<EvidenceRef>, LoopError>;

    /// Verify a hypothesis against collected evidence.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn verify_hypothesis(
        &self,
        node_id: &str,
        hypothesis: &str,
        ctx: &StepContext,
    ) -> Result<VerificationResult, LoopError>;
}

// ── Strategy ─────────────────────────────────────────────────────────────────

/// ITT exploration loop.
///
/// Each step explores the next unexplored node: collect evidence,
/// verify the hypothesis, and optionally expand to child hypotheses.
/// Halts when no unexplored nodes remain.
pub struct IttStrategy<E> {
    executor: E,
    name: &'static str,
}

impl<E: IttExecutor> IttStrategy<E> {
    /// Create a strategy wrapping the given executor.
    #[must_use]
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            name: "ITT",
        }
    }

    /// Override the strategy name.
    #[must_use]
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }
}

#[async_trait]
impl<E: IttExecutor> Strategy for IttStrategy<E> {
    type State = InvestigationTaskTree;
    type Output = InvestigationTaskTree;

    async fn step(
        &self,
        mut tree: InvestigationTaskTree,
        ctx: &StepContext,
    ) -> Result<Outcome<InvestigationTaskTree, InvestigationTaskTree>, LoopError> {
        // Priority 1: active branch has an unverified node (covers the root, which starts
        // in `explored` not `unexplored`).
        // Priority 2: pick the next node from the unexplored queue.
        let node_id = if let Some(ref ab) = tree.active_branch.clone() {
            let needs_work = tree.find_node(ab).is_none_or(|n| n.verification.is_none());
            if needs_work {
                ab.clone()
            } else {
                match tree.next_unexplored().cloned() {
                    Some(next) => {
                        tree.set_active(&next);
                        next
                    }
                    None => return Ok(Outcome::Halt(tree)),
                }
            }
        } else {
            match tree.next_unexplored().cloned() {
                Some(next) => {
                    tree.set_active(&next);
                    next
                }
                None => return Ok(Outcome::Halt(tree)),
            }
        };
        let hypothesis = tree
            .find_node(&node_id)
            .map(|n| n.hypothesis.clone())
            .unwrap_or_default();

        // Collect evidence.
        let evidence_refs = self
            .executor
            .collect_evidence(&node_id, &hypothesis, ctx)
            .await?;
        for ev in evidence_refs {
            tree.attach_evidence(&node_id, ev);
        }

        // Verify the hypothesis.
        let verification = self
            .executor
            .verify_hypothesis(&node_id, &hypothesis, ctx)
            .await?;
        let confidence = verification.confidence;
        tree.set_verification(&node_id, verification);
        tree.update_confidence(&node_id, confidence);

        // Expand to child hypotheses if confidence warrants exploration.
        if confidence >= 0.3 {
            let children = self.executor.expand(&node_id, &hypothesis, ctx).await?;
            for (child_hyp, child_conf) in children {
                tree.add_hypothesis(&node_id, child_hyp, QPhase::Theorize, child_conf);
            }
        }

        Ok(Outcome::Continue(tree))
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use futures_util::StreamExt as _;

    use crate::agent::{
        ChainContext,
        loops::{Budget, LoopRunner, Outcome},
    };

    use super::*;

    struct LeafOnlyExecutor;

    #[async_trait::async_trait]
    impl IttExecutor for LeafOnlyExecutor {
        async fn expand(
            &self,
            _node_id: &str,
            _hypothesis: &str,
            _ctx: &StepContext,
        ) -> Result<Vec<(String, f64)>, LoopError> {
            Ok(Vec::new()) // no children — leaf-only
        }

        async fn collect_evidence(
            &self,
            _node_id: &str,
            _hypothesis: &str,
            _ctx: &StepContext,
        ) -> Result<Vec<EvidenceRef>, LoopError> {
            Ok(vec![EvidenceRef {
                id: "ev-1".into(),
                path: "/tmp/app.log".into(),
                description: "OOM at 14:23".into(),
                collected_by: QPhase::Sweep,
            }])
        }

        async fn verify_hypothesis(
            &self,
            node_id: &str,
            hypothesis: &str,
            _ctx: &StepContext,
        ) -> Result<VerificationResult, LoopError> {
            Ok(VerificationResult {
                confirmed: true,
                evidence_ids: vec!["ev-1".into()],
                confidence: 0.8,
                conclusion: format!("{node_id}: {hypothesis} confirmed"),
            })
        }
    }

    #[tokio::test]
    async fn itt_explores_root_and_halts() {
        let tree = InvestigationTaskTree::new("case-001", "Server timeout under load");
        let runner = LoopRunner::new(IttStrategy::new(LeafOnlyExecutor), Budget::unlimited());
        let mut stream = runner.run(tree, ChainContext::default(), None);

        let mut halted = false;
        while let Some(result) = stream.next().await {
            if let Outcome::Halt(final_tree) = result.unwrap().outcome {
                // Root should be verified with evidence.
                let root = &final_tree.root;
                assert!(root.verification.is_some());
                assert_eq!(root.evidence.len(), 1);
                assert!((root.confidence - 0.8).abs() < f64::EPSILON);
                halted = true;
            }
        }
        assert!(halted);
    }

    #[test]
    fn tree_add_and_rank_hypotheses() {
        let mut tree = InvestigationTaskTree::new("case-002", "Root");
        let root_id = tree.root.id.clone();
        tree.add_hypothesis(&root_id, "H-high", QPhase::Theorize, 0.9);
        tree.add_hypothesis(&root_id, "H-low", QPhase::Theorize, 0.2);

        let ranked = tree.ranked_hypotheses();
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].hypothesis, "H-high");
    }

    #[test]
    fn tree_prune_and_recover() {
        let mut tree = InvestigationTaskTree::new("case-003", "Root");
        let root_id = tree.root.id.clone();
        let h_id = tree
            .add_hypothesis(&root_id, "Wrong", QPhase::Theorize, 0.1)
            .unwrap();

        assert!(tree.prune(&h_id));
        let leaves = tree.leaves();
        // Only root is a leaf (the pruned child doesn't count).
        assert_eq!(leaves.len(), 1);

        assert!(tree.recover(&h_id));
        assert_eq!(tree.leaves().len(), 1); // root has a child now, so root is no longer a leaf
    }

    #[test]
    fn linear_score_is_capped_at_one() {
        assert!((InvestigationTaskTree::linear_score(0, false, 0) - 0.0).abs() < f64::EPSILON);
        assert!(InvestigationTaskTree::linear_score(3, true, 3) <= 1.0);
        let max = InvestigationTaskTree::linear_score(10, true, 10);
        assert!((max - 1.0).abs() < f64::EPSILON);
    }
}
