//! Prompt builders that weave a [`TaskContract`] into both the worker's and
//! the supervisor's system messages.
//!
//! This is the **load-bearing prompt-engineering layer**. The contract's
//! criteria appear in two places with identical wording:
//!
//! 1. [`build_worker_prompt`] — adds a "HARD CONSTRAINTS" block to the user
//!    prompt so the LLM optimises for the same rubric it will be scored on.
//! 2. [`build_evaluator_prompt`] — produces a structured scoring prompt that
//!    asks the LLM to return a strict JSON verdict against the same criteria.
//!
//! Identical vocabulary between worker and supervisor closes the
//! "scored on something you weren't told about" failure mode.
//!
//! # Iteration discipline
//!
//! When [`build_worker_prompt`] is called with `prior_feedback = Some(s)`, the
//! feedback is woven in BEFORE the constraints — so the LLM reads
//! "previously you failed X, here's how to fix it" before re-reading the
//! constraint list. This converges faster than appending feedback at the end.

use crate::lightsquad::contract::TaskContract;

/// Build the worker-side prompt:
/// - Optional prior-iteration feedback (placed first when present)
/// - The original task prompt (user intent)
/// - A HARD CONSTRAINTS block built verbatim from the contract dimensions
/// - A reminder that the supervisor will score against these exact criteria
///
/// `iteration` is 0-based. `prior_feedback` carries the supervisor's
/// `Decision::Refine(...)` string from the previous round.
#[must_use]
pub fn build_worker_prompt(
    base_prompt: &str,
    contract: &TaskContract,
    iteration: u32,
    prior_feedback: Option<&str>,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(base_prompt.len() + 4_096);

    // ── Iteration banner ───────────────────────────────────────────────────
    if iteration > 0 {
        let _ = writeln!(
            out,
            "## ITERATION {} OF {} — RESPOND TO SUPERVISOR FEEDBACK\n",
            iteration + 1,
            contract.max_iterations
        );
        if let Some(feedback) = prior_feedback {
            let _ = writeln!(
                out,
                "The previous iteration's artifact did NOT meet the contract. \
                 The supervisor reported the following BLOCKING gaps. \
                 You MUST address each one before satisfying the original task:\n"
            );
            let _ = writeln!(out, "{feedback}\n");
            let _ = writeln!(
                out,
                "---\nNow proceed with the original task, with the above gaps \
                 fixed AND the HARD CONSTRAINTS below satisfied.\n"
            );
        }
    }

    // ── Contract header ───────────────────────────────────────────────────
    let _ = writeln!(
        out,
        "## CONTRACT — {} (artifact kind: {})\n",
        contract.task_id, contract.artifact_kind
    );
    if let Some(ns) = &contract.northstar {
        let _ = writeln!(
            out,
            "Northstar tie-in: pillar {} — {}\n",
            ns.pillar, ns.delta_claim
        );
    }

    // ── HARD CONSTRAINTS — verbatim from contract.dimensions[].criteria ───
    let _ = writeln!(
        out,
        "## HARD CONSTRAINTS — your output will be scored against these by an \
         independent supervisor. Each constraint is non-negotiable; failing \
         any one drops that dimension's score below the {:.0}% acceptance \
         threshold and triggers a redeploy.\n",
        contract.confidence_threshold * 100.0
    );
    for (i, dim) in contract.dimensions.iter().enumerate() {
        let _ = writeln!(
            out,
            "### Dimension {} of {} — `{}` (weight: {:.2})",
            i + 1,
            contract.dimensions.len(),
            dim.name,
            dim.weight
        );
        for (j, criterion) in dim.criteria.iter().enumerate() {
            let _ = writeln!(out, "  {}.{}.  {}", i + 1, j + 1, criterion);
        }
        if let Some(hint) = &dim.scoring_hint {
            let _ = writeln!(out, "  Scoring hint for the supervisor: {hint}");
        }
        let _ = writeln!(out);
    }

    // ── Original task prompt ──────────────────────────────────────────────
    let _ = writeln!(out, "## ORIGINAL TASK\n\n{base_prompt}\n");

    // ── Reminder ──────────────────────────────────────────────────────────
    let _ = writeln!(
        out,
        "## REMINDER\n\n\
         Emit your output in the standard `## File: <path>` + fenced block + \
         `## Commit:` format. Place the artifact at `{}` exactly. The \
         supervisor will read it from there and score it against the {} \
         dimensions above. Do not add criteria beyond the contract; do not \
         skip criteria within it.",
        contract.artifact_path,
        contract.dimensions.len()
    );

    out
}

/// Build the supervisor-side prompt — asks the LLM to score the artifact
/// against the contract and return a strict JSON verdict.
///
/// `source_of_truth` is the trusted reference data (architecture facts,
/// allowlists, the original plan excerpt) the supervisor uses to detect
/// hallucinations. Pass an empty string when no out-of-band truth is needed.
#[must_use]
pub fn build_evaluator_prompt(
    contract: &TaskContract,
    artifact: &str,
    source_of_truth: &str,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(artifact.len() + 4_096);

    let _ = writeln!(
        out,
        "You are a contract supervisor. Score the following artifact against \
         the dimensions below and return a STRICT JSON verdict — nothing \
         else, no prose, no markdown fences around the JSON."
    );
    let _ = writeln!(
        out,
        "\n## CONTRACT — task: {} (kind: {}, expected path: {})\n",
        contract.task_id, contract.artifact_kind, contract.artifact_path
    );
    if let Some(ns) = &contract.northstar {
        let _ = writeln!(
            out,
            "Northstar tie-in: pillar {} — {}\n",
            ns.pillar, ns.delta_claim
        );
    }

    for (i, dim) in contract.dimensions.iter().enumerate() {
        let _ = writeln!(
            out,
            "### Dimension {} — `{}` (weight: {:.2})",
            i + 1,
            dim.name,
            dim.weight
        );
        for (j, criterion) in dim.criteria.iter().enumerate() {
            let _ = writeln!(out, "  {}.{}.  {}", i + 1, j + 1, criterion);
        }
        if let Some(hint) = &dim.scoring_hint {
            let _ = writeln!(out, "  Scoring hint: {hint}");
        }
        let _ = writeln!(out);
    }

    if !source_of_truth.is_empty() {
        let _ = writeln!(
            out,
            "## SOURCE OF TRUTH — use this to detect hallucinations + verify \
             specifics. Any name in the artifact NOT in this list is a \
             hallucination candidate for the `no_hallucination` dimension.\n\n\
             {source_of_truth}\n"
        );
    }

    let _ = writeln!(out, "## ARTIFACT TO SCORE\n\n```\n{artifact}\n```\n");

    // ── Strict JSON schema ────────────────────────────────────────────────
    let _ = writeln!(
        out,
        "## OUTPUT — return EXACTLY this JSON structure, with no surrounding \
         text. Use `ci_low` and `ci_high` to express your uncertainty:\n"
    );
    let _ = writeln!(out, "{}", json_schema_example(contract));

    let _ = writeln!(
        out,
        "\n## SCORING DISCIPLINE\n\
         - `score` is your best point estimate (0.0–1.0).\n\
         - `ci_low` is the lower bound — be conservative. If you cannot \
           verify a criterion from the artifact alone, the CI must widen.\n\
         - `ci_high` is the upper bound.\n\
         - `failed_criteria` lists the exact criterion strings (verbatim) \
           that the artifact does not satisfy. Empty when dimension passes.\n\
         - `reasoning` quotes the artifact directly — no hand-waving. The \
           worker will read this string verbatim on the next iteration."
    );

    out
}

fn json_schema_example(contract: &TaskContract) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "{{");
    let _ = writeln!(out, "  \"per_dimension\": [");
    for (i, dim) in contract.dimensions.iter().enumerate() {
        let comma = if i + 1 < contract.dimensions.len() {
            ","
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "    {{\"name\": \"{}\", \"score\": 0.0, \"ci_low\": 0.0, \"ci_high\": 0.0, \"reasoning\": \"...\", \"failed_criteria\": []}}{comma}",
            dim.name
        );
    }
    let _ = writeln!(out, "  ]");
    let _ = writeln!(out, "}}");
    out
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::lightsquad::contract::html_diagram_contract;

    /// The worker prompt MUST include every criterion verbatim — that's the
    /// contract guarantee. The supervisor uses the same wording.
    #[test]
    fn worker_prompt_includes_every_criterion_verbatim() {
        let contract = html_diagram_contract("t", "x.html");
        let prompt = build_worker_prompt("regenerate the diagram", &contract, 0, None);

        // Every criterion string must appear character-for-character.
        for dim in &contract.dimensions {
            for criterion in &dim.criteria {
                assert!(
                    prompt.contains(criterion),
                    "criterion missing from worker prompt: {criterion}"
                );
            }
        }
    }

    /// Iteration 0 with no feedback skips the iteration banner.
    #[test]
    fn worker_prompt_omits_iteration_banner_on_first_call() {
        let contract = html_diagram_contract("t", "x.html");
        let prompt = build_worker_prompt("do the thing", &contract, 0, None);
        assert!(
            !prompt.contains("ITERATION"),
            "first iteration should not show a banner"
        );
    }

    /// Iteration ≥1 with feedback weaves the prior critique BEFORE the
    /// constraints so the LLM sees the failure context first.
    #[test]
    fn worker_prompt_weaves_prior_feedback_before_constraints() {
        let contract = html_diagram_contract("t", "x.html");
        let prompt = build_worker_prompt(
            "do the thing",
            &contract,
            1,
            Some("topology: missing edges between L1 nodes"),
        );
        assert!(prompt.contains("ITERATION 2"));
        assert!(prompt.contains("topology: missing edges"));
        let feedback_pos = prompt.find("topology: missing edges").unwrap();
        let constraints_pos = prompt.find("HARD CONSTRAINTS").unwrap();
        assert!(
            feedback_pos < constraints_pos,
            "prior feedback must appear before the constraints block"
        );
    }

    /// The evaluator prompt embeds the artifact + source of truth and asks
    /// for a strict JSON output schema.
    #[test]
    fn evaluator_prompt_embeds_artifact_and_schema() {
        let contract = html_diagram_contract("t", "x.html");
        let artifact = "<!DOCTYPE html><html><body>test</body></html>";
        let truth = "Known components: gateway, webshell, CORSO, EVA";

        let prompt = build_evaluator_prompt(&contract, artifact, truth);
        assert!(prompt.contains(artifact), "artifact must be embedded");
        assert!(prompt.contains(truth), "source of truth must be embedded");
        assert!(prompt.contains("\"per_dimension\""), "JSON schema marker");
        assert!(
            prompt.contains("ci_low"),
            "evaluator must be asked for CI bounds, not just point estimate"
        );

        // Every dimension's name must appear in the schema example
        for dim in &contract.dimensions {
            assert!(
                prompt.contains(&dim.name),
                "dimension '{}' missing from evaluator schema",
                dim.name
            );
        }
    }

    /// Empty source-of-truth omits the SOURCE OF TRUTH block (no spurious
    /// section header with no content).
    #[test]
    fn evaluator_prompt_omits_truth_block_when_empty() {
        let contract = html_diagram_contract("t", "x.html");
        let prompt = build_evaluator_prompt(&contract, "irrelevant", "");
        assert!(
            !prompt.contains("SOURCE OF TRUTH"),
            "should not emit empty truth section"
        );
    }

    /// Worker and evaluator share identical criterion wording — this is the
    /// load-bearing guarantee. If they diverged, the worker would optimise
    /// for one rubric while being scored on another.
    #[test]
    fn worker_and_evaluator_share_identical_criterion_wording() {
        let contract = html_diagram_contract("t", "x.html");
        let worker = build_worker_prompt("base", &contract, 0, None);
        let evaluator = build_evaluator_prompt(&contract, "art", "truth");

        for dim in &contract.dimensions {
            for criterion in &dim.criteria {
                assert!(
                    worker.contains(criterion),
                    "criterion not in worker prompt: {criterion}"
                );
                assert!(
                    evaluator.contains(criterion),
                    "criterion not in evaluator prompt: {criterion}"
                );
            }
        }
    }
}
