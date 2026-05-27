//! Per-task LASDLC contract — the shared vocabulary between worker and supervisor.
//!
//! A [`TaskContract`] is a compact subset of LASDLC v2.5.1's deliverable
//! benchmark, scoped to a single artifact. It is the **load-bearing structure**
//! that closes the prompt-engineering loop:
//!
//! 1. **Worker side** — the contract's `dimensions[].criteria` are woven into
//!    the worker's system prompt as HARD CONSTRAINTS. The LLM optimises for
//!    them because they're explicit.
//! 2. **Supervisor side** — the same criteria become the scoring rubric. The
//!    contract guarantees the worker and supervisor share an identical
//!    vocabulary; there is no "the LLM was scored on something it wasn't told
//!    about" failure mode.
//! 3. **Iteration** — when [`Verdict::decision`] is [`Decision::Refine`], the
//!    feedback string is appended to the next iteration's prompt, telling the
//!    LLM *exactly* which criteria failed and why.
//!
//! # Why this is the load-bearing fix
//!
//! The v2 diagram lost topology, hallucinated nodes, and inverted layer
//! assignments because the original prompt asked for "style preservation"
//! without declaring "every component name must trace to source" or
//! "topology requires directed edges." The model produced what it was scored
//! on. The contract makes the scoring explicit and shared.
//!
//! # Calibration
//!
//! Scores carry confidence intervals (`ci_low`, `ci_high`). The
//! [`Decision::Accept`] gate uses **`ci_low ≥ confidence_threshold`**, not the
//! point estimate. A noisy 0.96 ± 0.20 does not pass; a tight 0.96 ± 0.02 does.
//! This protects against the supervisor scoring high on a borderline artifact
//! the operator would reject.

use serde::{Deserialize, Serialize};

// ── Contract schema ──────────────────────────────────────────────────────────

/// One per task. Compact LASDLC subset that drives the prompt + supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContract {
    /// Task identifier (matches `Task::id`).
    pub task_id: String,
    /// Free-form artifact category (e.g. `"html_diagram"`, `"rust_crate"`,
    /// `"markdown_doc"`). Used by the supervisor to select scoring heuristics
    /// when criteria do not fully specify them.
    pub artifact_kind: String,
    /// Worktree-relative path of the artifact the worker is expected to
    /// produce. Used to read the artifact for supervisor evaluation.
    pub artifact_path: String,
    /// Optional Northstar tie-in (pillar + delta claim). When present,
    /// surfaces in the prompt so the LLM understands what the artifact is for.
    #[serde(default)]
    pub northstar: Option<NorthstarTieIn>,
    /// Weighted scoring dimensions. Order is preserved in the prompt so the
    /// author can place the highest-weight constraint first.
    pub dimensions: Vec<Dimension>,
    /// `ci_low` of the weighted aggregate must reach this for
    /// [`Decision::Accept`]. Default 0.95.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Maximum number of redeploy iterations before escalating to HITL.
    /// Default 5.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

/// Northstar context surfaced to both worker and supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NorthstarTieIn {
    /// Pillar identifier (`"P1"` through `"P7"`).
    pub pillar: String,
    /// Verifiable delta this artifact contributes.
    pub delta_claim: String,
}

/// One scoring dimension. The worker treats `criteria` as hard constraints;
/// the supervisor treats them as scoring rubric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimension {
    /// Short `snake_case` identifier (e.g. `"topology_fidelity"`). Used in
    /// reasoning logs and verdict JSON.
    pub name: String,
    /// Weight in the aggregate. Need not sum to 1.0 — the supervisor
    /// normalises by `Σ weights`.
    pub weight: f64,
    /// Bullet criteria — each is a single checkable statement. The worker
    /// sees these as HARD CONSTRAINTS; the supervisor scores against them.
    pub criteria: Vec<String>,
    /// Optional scoring hint for the supervisor (e.g. "count <svg> elements;
    /// 0 = score 0, 5+ with labels = score 1.0"). Bypasses the LLM's
    /// freeform interpretation when the rubric is deterministic.
    #[serde(default)]
    pub scoring_hint: Option<String>,
}

const fn default_confidence_threshold() -> f64 {
    0.95
}

const fn default_max_iterations() -> u32 {
    5
}

// ── Verdict ──────────────────────────────────────────────────────────────────

/// Aggregate verdict from the supervisor — what comes back from a single
/// evaluation pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    /// Per-dimension scores in the same order as `TaskContract::dimensions`.
    pub per_dimension: Vec<DimensionScore>,
    /// Weighted aggregate point estimate, `Σ(w × score) / Σ(w)`.
    pub weighted_score: f64,
    /// Weighted aggregate `ci_low` — this is the value that gates accept.
    pub weighted_ci_low: f64,
    /// Weighted aggregate `ci_high`.
    pub weighted_ci_high: f64,
    /// What the orchestrator does next.
    pub decision: Decision,
}

/// One dimension's evaluator output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    /// Matches [`Dimension::name`].
    pub name: String,
    /// Point estimate, 0.0–1.0.
    pub score: f64,
    /// Lower bound of the confidence interval, 0.0–1.0.
    pub ci_low: f64,
    /// Upper bound of the confidence interval, 0.0–1.0.
    pub ci_high: f64,
    /// Specific evidence — quote the artifact, name the criterion that
    /// failed, no hand-waving. This is what feeds back into the next
    /// iteration's prompt.
    pub reasoning: String,
    /// Criteria from the contract dimension that the artifact did NOT meet.
    /// Used to build a targeted feedback prompt.
    #[serde(default)]
    pub failed_criteria: Vec<String>,
}

/// What the orchestrator does next based on [`Verdict::weighted_ci_low`] and
/// the iteration count.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "feedback")]
pub enum Decision {
    /// `ci_low ≥ confidence_threshold` — commit + merge.
    Accept,
    /// Score below threshold — redeploy with this structured feedback.
    Refine(String),
    /// Max iterations reached — push to HITL queue.
    Escalate,
}

impl Verdict {
    /// Construct a [`Verdict`] from per-dimension scores + the contract.
    /// Computes weighted aggregates and decides the action based on
    /// `confidence_threshold` and `iteration < max_iterations`.
    ///
    /// `feedback_builder` is invoked when the verdict requires
    /// [`Decision::Refine`] — it receives the failing dimensions and
    /// produces the targeted feedback string.
    #[must_use]
    pub fn from_dimensions(
        per_dimension: Vec<DimensionScore>,
        contract: &TaskContract,
        iteration: u32,
        feedback_builder: impl FnOnce(&[DimensionScore]) -> String,
    ) -> Self {
        let mut total_weight = 0.0_f64;
        let mut weighted_score = 0.0_f64;
        let mut weighted_ci_low = 0.0_f64;
        let mut weighted_ci_high = 0.0_f64;

        for dim_score in &per_dimension {
            let weight = contract
                .dimensions
                .iter()
                .find(|d| d.name == dim_score.name)
                .map_or(0.0, |d| d.weight);
            total_weight += weight;
            weighted_score += weight * dim_score.score;
            weighted_ci_low += weight * dim_score.ci_low;
            weighted_ci_high += weight * dim_score.ci_high;
        }

        if total_weight > 0.0 {
            weighted_score /= total_weight;
            weighted_ci_low /= total_weight;
            weighted_ci_high /= total_weight;
        }

        let decision = if weighted_ci_low >= contract.confidence_threshold {
            Decision::Accept
        } else if iteration + 1 >= contract.max_iterations {
            Decision::Escalate
        } else {
            let failing: Vec<DimensionScore> = per_dimension
                .iter()
                .filter(|d| d.ci_low < contract.confidence_threshold)
                .cloned()
                .collect();
            Decision::Refine(feedback_builder(&failing))
        };

        Self {
            per_dimension,
            weighted_score,
            weighted_ci_low,
            weighted_ci_high,
            decision,
        }
    }

    /// `true` when the verdict permits commit + merge.
    #[must_use]
    pub const fn is_accept(&self) -> bool {
        matches!(self.decision, Decision::Accept)
    }
}

// ── Built-in contract templates ──────────────────────────────────────────────

/// HTML architecture-diagram contract targeting the five gaps observed in
/// the v2 regen (topology, hallucinations, layer correctness, information
/// density, nesting). Use this as a starting point or override per-task.
#[must_use]
pub fn html_diagram_contract(
    task_id: impl Into<String>,
    artifact_path: impl Into<String>,
) -> TaskContract {
    TaskContract {
        task_id: task_id.into(),
        artifact_kind: "html_diagram".to_owned(),
        artifact_path: artifact_path.into(),
        northstar: Some(NorthstarTieIn {
            pillar: "P4".to_owned(),
            delta_claim: "Operator can identify any production component within 3 seconds.".to_owned(),
        }),
        dimensions: vec![
            Dimension {
                name: "topology_fidelity".to_owned(),
                weight: 0.30,
                criteria: vec![
                    "Every node carries at least one directed edge (SVG path, CSS-positioned connector, or explicit annotation) to another node. A flat grid with zero edges is a hard fail.".to_owned(),
                    "Cross-layer edges carry a protocol annotation (e.g. 'stdio JSON-RPC 2.0', 'HTTP :8733 + WS + SSE', 'HTTPS REST', 'HTTP proxy :8080').".to_owned(),
                    "Sub-components nested INSIDE a parent process box appear visually contained (positioned within the parent's bounding box), not as siblings at the same grid level.".to_owned(),
                ],
                scoring_hint: Some(
                    "Count distinct edge elements: 0 → score 0.0, 1-3 → 0.3, 4-9 → 0.6, 10+ with protocol labels → 0.9-1.0. Penalise if any cross-layer edge lacks a protocol annotation.".to_owned(),
                ),
            },
            Dimension {
                name: "no_hallucination".to_owned(),
                weight: 0.25,
                criteria: vec![
                    "Every component name must trace to a workspace crate, a known sibling binary, or an entity in the provided architecture facts. Invented components are a hard fail.".to_owned(),
                    "Every port number must trace to an actual bind site in the architecture facts.".to_owned(),
                    "Every binary path must match the form documented in the architecture facts.".to_owned(),
                ],
                scoring_hint: Some(
                    "List every component name in the artifact. For each, mark whether it appears in the provided allowlist. Score = (recognised / total). Any unknown component = score capped at 0.7.".to_owned(),
                ),
            },
            Dimension {
                name: "layer_correctness".to_owned(),
                weight: 0.15,
                criteria: vec![
                    "L0 = external actors only (humans, external systems like Ollama Cloud, GitHub).".to_owned(),
                    "L1 = process boundaries (binaries). The six sibling MCP binaries (CORSO, EVA, SOUL, QUANTUM, SERAPH, LÆX) are L1 because each is a separate subprocess. AYIN is L1 because it is an HTTP daemon process.".to_owned(),
                    "L2 = components inside a process (e.g. Conductor inside the gateway).".to_owned(),
                    "L3 = modules / file boundaries. L4 = function/data. L5 = dependencies. L6 = runtime/IPC. L7 = UI surfaces.".to_owned(),
                    "Do not invent new layer semantics or renumber layers.".to_owned(),
                ],
                scoring_hint: Some(
                    "Check the placement of each known entity against the rules. Sibling at L6 or AYIN at L2 = -0.2 each.".to_owned(),
                ),
            },
            Dimension {
                name: "information_density".to_owned(),
                weight: 0.15,
                criteria: vec![
                    "Every L1 binary node displays its binary path (e.g. '~/.lightarchitects/bin/lightarchitects').".to_owned(),
                    "Every L1 binary that listens on a port displays the port (e.g. ':8733', ':3742').".to_owned(),
                    "Every L1 binary displays its primary protocol (e.g. 'stdio JSON-RPC 2.0', 'HTTP', 'HTTPS').".to_owned(),
                    "Generic labels like 'Gateway · MCP Server' without specifics are insufficient.".to_owned(),
                ],
                scoring_hint: Some(
                    "For each L1 node, check path + port + protocol presence. Score = (specifics_shown / specifics_expected).".to_owned(),
                ),
            },
            Dimension {
                name: "style_fidelity".to_owned(),
                weight: 0.15,
                criteria: vec![
                    "Preserves the CSS palette from the supplied style excerpt (--amber, --blue, --green, --violet, --coral, --slate variables).".to_owned(),
                    "Preserves the typography stack (Lexend, DM Mono, Space Grotesk).".to_owned(),
                    "Preserves the header badge convention (uppercase tracked, monospace).".to_owned(),
                ],
                scoring_hint: None,
            },
        ],
        confidence_threshold: 0.95,
        max_iterations: 5,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]
mod tests {
    use super::*;

    fn dim(name: &str, score: f64, ci_low: f64, ci_high: f64) -> DimensionScore {
        DimensionScore {
            name: name.to_owned(),
            score,
            ci_low,
            ci_high,
            reasoning: "test".to_owned(),
            failed_criteria: vec![],
        }
    }

    fn contract_with_weights(weights: &[(&str, f64)]) -> TaskContract {
        TaskContract {
            task_id: "t".to_owned(),
            artifact_kind: "x".to_owned(),
            artifact_path: "x".to_owned(),
            northstar: None,
            dimensions: weights
                .iter()
                .map(|(name, w)| Dimension {
                    name: (*name).to_owned(),
                    weight: *w,
                    criteria: vec![],
                    scoring_hint: None,
                })
                .collect(),
            confidence_threshold: 0.95,
            max_iterations: 5,
        }
    }

    /// All dims pass with tight CIs → ACCEPT.
    #[test]
    fn verdict_accepts_when_ci_low_above_threshold() {
        let contract = contract_with_weights(&[("a", 0.5), ("b", 0.5)]);
        let dims = vec![dim("a", 0.97, 0.96, 0.99), dim("b", 0.97, 0.96, 0.99)];
        let v = Verdict::from_dimensions(dims, &contract, 0, |_| "n/a".to_owned());
        assert!(v.is_accept(), "expected Accept, got {:?}", v.decision);
    }

    /// Point estimate is high but CI lower bound dips below threshold → REFINE.
    /// This is the calibration guard — a noisy "looks good" doesn't pass.
    #[test]
    fn verdict_refines_when_ci_low_dips_below_threshold() {
        let contract = contract_with_weights(&[("a", 1.0)]);
        // Point 0.96 looks accept-worthy, but CI low is 0.70 — noisy.
        let dims = vec![dim("a", 0.96, 0.70, 0.99)];
        let v = Verdict::from_dimensions(dims, &contract, 0, |_| "needs more evidence".to_owned());
        assert!(
            matches!(v.decision, Decision::Refine(_)),
            "expected Refine on wide CI, got {:?}",
            v.decision
        );
    }

    /// Score below threshold AND on the last allowed iteration → ESCALATE.
    #[test]
    fn verdict_escalates_after_max_iterations() {
        let mut contract = contract_with_weights(&[("a", 1.0)]);
        contract.max_iterations = 3;
        let dims = vec![dim("a", 0.50, 0.40, 0.60)];
        // Iteration 2 → next is 3 == max_iterations → ESCALATE.
        let v = Verdict::from_dimensions(dims, &contract, 2, |_| String::new());
        assert!(
            matches!(v.decision, Decision::Escalate),
            "expected Escalate at iter+1 == max, got {:?}",
            v.decision
        );
    }

    /// Weighted aggregation respects per-dimension weights.
    #[test]
    fn verdict_weighted_aggregate_is_correct() {
        let contract = contract_with_weights(&[("a", 3.0), ("b", 1.0)]);
        let dims = vec![dim("a", 1.0, 1.0, 1.0), dim("b", 0.0, 0.0, 0.0)];
        let v = Verdict::from_dimensions(dims, &contract, 0, |_| String::new());
        // (3 × 1.0 + 1 × 0.0) / (3 + 1) = 0.75
        assert!((v.weighted_score - 0.75).abs() < 1e-9);
    }

    /// Feedback builder receives only the dimensions whose `ci_low` is below
    /// the threshold — passing dims are not in the failure list.
    #[test]
    fn verdict_feedback_receives_only_failing_dimensions() {
        let contract = contract_with_weights(&[("good", 0.5), ("bad", 0.5)]);
        let dims = vec![dim("good", 0.99, 0.98, 1.0), dim("bad", 0.30, 0.20, 0.40)];
        let v = Verdict::from_dimensions(dims, &contract, 0, |failing| {
            assert_eq!(failing.len(), 1);
            assert_eq!(failing[0].name, "bad");
            format!("only {} failing", failing.len())
        });
        match v.decision {
            Decision::Refine(s) => assert_eq!(s, "only 1 failing"),
            other => panic!("expected Refine, got {other:?}"),
        }
    }

    /// The built-in HTML diagram contract has the 5 expected dimensions
    /// covering the gaps we observed in v2.
    #[test]
    fn html_diagram_contract_covers_observed_v2_gaps() {
        let c = html_diagram_contract("t", "x.html");
        let names: Vec<&str> = c.dimensions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"topology_fidelity"));
        assert!(names.contains(&"no_hallucination"));
        assert!(names.contains(&"layer_correctness"));
        assert!(names.contains(&"information_density"));
        assert!(names.contains(&"style_fidelity"));

        // Topology is the heaviest weight — it was the worst gap in v2.
        let topo = c
            .dimensions
            .iter()
            .find(|d| d.name == "topology_fidelity")
            .unwrap();
        let max_other_weight = c
            .dimensions
            .iter()
            .filter(|d| d.name != "topology_fidelity")
            .map(|d| d.weight)
            .fold(0.0_f64, f64::max);
        assert!(topo.weight >= max_other_weight);
    }

    /// Contract round-trips through YAML cleanly.
    #[test]
    fn task_contract_yaml_roundtrip() {
        let original = html_diagram_contract("diagram-regen", "la-platform-diagram.html");
        let yaml = serde_yaml::to_string(&original).unwrap();
        let parsed: TaskContract = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.task_id, original.task_id);
        assert_eq!(parsed.dimensions.len(), original.dimensions.len());
    }
}
