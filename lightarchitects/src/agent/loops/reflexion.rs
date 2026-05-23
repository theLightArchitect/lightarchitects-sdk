//! Reflexion strategy — SDK port of QUANTUM agentic/reflexion.rs.
//!
//! Post-investigation self-reflection lifecycle: Provisional → Reviewed → Verified → Archived.
//! Only Verified entries become authoritative knowledge safe to propagate to the helix.
//!
//! Source: Shinn et al. 2023 (`NeurIPS`), "Reflexion: Language Agents with Verbal Reinforcement Learning"

use std::fmt::Write as _;

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Types (ported from QUANTUM) ───────────────────────────────────────────────

/// Reflexion entry lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflexionState {
    /// Written after CLOSE — may contain premature conclusions.
    Provisional,
    /// Human or reviewer has seen it (HITL gate passed).
    Reviewed,
    /// Confirmed correct by subsequent investigation or explicit review.
    Verified,
    /// Historical — no longer actively consulted but preserved.
    Archived,
}

impl std::fmt::Display for ReflexionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Provisional => "PROVISIONAL",
            Self::Reviewed => "REVIEWED",
            Self::Verified => "VERIFIED",
            Self::Archived => "ARCHIVED",
        })
    }
}

/// A reflexion entry — what was learned from an investigation.
#[derive(Debug, Clone)]
pub struct ReflexionEntry {
    /// Case or task identifier.
    pub case_id: String,
    /// Current lifecycle state.
    pub state: ReflexionState,
    /// New patterns discovered in this investigation.
    pub new_patterns: Vec<String>,
    /// Prior knowledge that applied successfully.
    pub applied_knowledge: Vec<String>,
    /// Identified root cause, if found.
    pub root_cause: Option<String>,
    /// Improvements for next time.
    pub improvements: Vec<String>,
    /// Confidence in the reflexion (0.0–1.0).
    pub confidence: f64,
}

impl ReflexionEntry {
    /// Promote to the next lifecycle state.
    ///
    /// Returns `false` if already at `Archived` (terminal state).
    pub fn promote(&mut self) -> bool {
        match self.state {
            ReflexionState::Provisional => {
                self.state = ReflexionState::Reviewed;
                true
            }
            ReflexionState::Reviewed => {
                self.state = ReflexionState::Verified;
                true
            }
            ReflexionState::Verified => {
                self.state = ReflexionState::Archived;
                true
            }
            ReflexionState::Archived => false,
        }
    }

    /// Returns `true` when this entry is authoritative (safe to publish to the knowledge graph).
    #[must_use]
    pub fn is_authoritative(&self) -> bool {
        self.state == ReflexionState::Verified
    }

    /// Format the entry as Markdown for helix storage.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut md = format!(
            "# Reflexion: {}\n\n**State:** {}\n**Confidence:** {:.0}%\n\n",
            self.case_id,
            self.state,
            self.confidence * 100.0
        );
        if let Some(ref cause) = self.root_cause {
            let _ = write!(md, "## Root Cause\n{cause}\n\n");
        }
        if !self.new_patterns.is_empty() {
            md.push_str("## New Patterns\n");
            for p in &self.new_patterns {
                let _ = writeln!(md, "- {p}");
            }
            md.push('\n');
        }
        if !self.improvements.is_empty() {
            md.push_str("## Improvements\n");
            for imp in &self.improvements {
                let _ = writeln!(md, "- {imp}");
            }
            md.push('\n');
        }
        md
    }
}

// ── Loop state ────────────────────────────────────────────────────────────────

/// State threaded through each step of the [`ReflexionStrategy`] loop.
#[derive(Debug, Clone)]
pub struct ReflexionLoopState {
    /// Case or task identifier.
    pub case_id: String,
    /// Context string describing the completed investigation.
    pub context: String,
    /// The reflexion entry, populated after the first step.
    pub entry: Option<ReflexionEntry>,
    /// Maximum promotion rounds before forced halt.
    pub max_rounds: u32,
    /// Completed promotion rounds.
    pub round: u32,
}

impl ReflexionLoopState {
    /// Start a reflexion loop for the given case.
    #[must_use]
    pub fn new(case_id: impl Into<String>, context: impl Into<String>, max_rounds: u32) -> Self {
        Self {
            case_id: case_id.into(),
            context: context.into(),
            entry: None,
            max_rounds,
            round: 0,
        }
    }
}

// ── Executor ──────────────────────────────────────────────────────────────────

/// Decision returned by [`ReflexionExecutor::review`].
#[derive(Debug, Clone)]
pub struct ReflexionReview {
    /// Whether to promote the entry to the next lifecycle state.
    pub should_promote: bool,
    /// Additional improvements to add to the entry.
    pub improvements: Vec<String>,
    /// Confidence adjustment (+/- applied to the entry's confidence).
    pub confidence_delta: f64,
}

/// Provider-agnostic executor for the Reflexion lifecycle loop.
#[async_trait]
pub trait ReflexionExecutor: Send + Sync + 'static {
    /// Generate an initial [`ReflexionEntry`] from the investigation context.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn generate(
        &self,
        case_id: &str,
        context: &str,
        ctx: &StepContext,
    ) -> Result<ReflexionEntry, LoopError>;

    /// Review an existing entry and return a promotion decision.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn review(
        &self,
        entry: &ReflexionEntry,
        ctx: &StepContext,
    ) -> Result<ReflexionReview, LoopError>;
}

// ── Strategy ─────────────────────────────────────────────────────────────────

/// Reflexion lifecycle loop.
///
/// Step 1: generate a [`ReflexionEntry`] from context.
/// Subsequent steps: call [`ReflexionExecutor::review`] and promote the entry
/// until it reaches [`ReflexionState::Verified`] or `max_rounds` is exhausted.
pub struct ReflexionStrategy<E> {
    executor: E,
    name: &'static str,
}

impl<E: ReflexionExecutor> ReflexionStrategy<E> {
    /// Create a strategy wrapping the given executor.
    #[must_use]
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            name: "Reflexion",
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
impl<E: ReflexionExecutor> Strategy for ReflexionStrategy<E> {
    type State = ReflexionLoopState;
    type Output = ReflexionEntry;

    async fn step(
        &self,
        mut state: ReflexionLoopState,
        ctx: &StepContext,
    ) -> Result<Outcome<ReflexionLoopState, ReflexionEntry>, LoopError> {
        // Step 1: generate the initial entry.
        let Some(ref mut entry) = state.entry else {
            let generated = self
                .executor
                .generate(&state.case_id, &state.context, ctx)
                .await?;
            state.entry = Some(generated);
            return Ok(Outcome::Continue(state));
        };

        // Halt if already verified (authoritative) or archived (terminal).
        if matches!(
            entry.state,
            ReflexionState::Verified | ReflexionState::Archived
        ) {
            return Ok(Outcome::Halt(state.entry.unwrap_or_else(|| {
                ReflexionEntry {
                    case_id: state.case_id.clone(),
                    state: ReflexionState::Archived,
                    new_patterns: Vec::new(),
                    applied_knowledge: Vec::new(),
                    root_cause: None,
                    improvements: Vec::new(),
                    confidence: 0.0,
                }
            })));
        }

        // Halt if max rounds exhausted.
        if state.round >= state.max_rounds {
            return Ok(Outcome::Halt(entry.clone()));
        }

        let review = self.executor.review(entry, ctx).await?;
        entry.improvements.extend(review.improvements);
        entry.confidence = (entry.confidence + review.confidence_delta).clamp(0.0, 1.0);
        if review.should_promote {
            entry.promote();
        }
        state.round += 1;

        if matches!(
            entry.state,
            ReflexionState::Verified | ReflexionState::Archived
        ) {
            Ok(Outcome::Halt(entry.clone()))
        } else {
            Ok(Outcome::Continue(state))
        }
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

    /// Always promotes with no improvements.
    struct AutoPromoteExecutor;

    #[async_trait::async_trait]
    impl ReflexionExecutor for AutoPromoteExecutor {
        async fn generate(
            &self,
            case_id: &str,
            _context: &str,
            _ctx: &StepContext,
        ) -> Result<ReflexionEntry, LoopError> {
            Ok(ReflexionEntry {
                case_id: case_id.into(),
                state: ReflexionState::Provisional,
                new_patterns: vec!["pattern A".into()],
                applied_knowledge: Vec::new(),
                root_cause: Some("memory leak".into()),
                improvements: Vec::new(),
                confidence: 0.7,
            })
        }

        async fn review(
            &self,
            _entry: &ReflexionEntry,
            _ctx: &StepContext,
        ) -> Result<ReflexionReview, LoopError> {
            Ok(ReflexionReview {
                should_promote: true,
                improvements: Vec::new(),
                confidence_delta: 0.05,
            })
        }
    }

    #[tokio::test]
    async fn reflexion_reaches_verified_state() {
        let runner = LoopRunner::new(
            ReflexionStrategy::new(AutoPromoteExecutor),
            Budget::unlimited(),
        );
        let mut stream = runner.run(
            ReflexionLoopState::new("case-001", "server timeout", 10),
            ChainContext::default(),
            None,
        );

        let mut final_entry = None;
        while let Some(step) = stream.next().await {
            if let Outcome::Halt(entry) = step.unwrap().outcome {
                final_entry = Some(entry);
            }
        }
        let entry = final_entry.unwrap();
        assert_eq!(entry.state, ReflexionState::Verified);
        assert!(entry.confidence > 0.7);
    }

    #[test]
    fn reflexion_entry_promotion_lifecycle() {
        let mut entry = ReflexionEntry {
            case_id: "c".into(),
            state: ReflexionState::Provisional,
            new_patterns: Vec::new(),
            applied_knowledge: Vec::new(),
            root_cause: None,
            improvements: Vec::new(),
            confidence: 0.5,
        };
        assert!(!entry.is_authoritative());
        assert!(entry.promote());
        assert_eq!(entry.state, ReflexionState::Reviewed);
        assert!(entry.promote());
        assert_eq!(entry.state, ReflexionState::Verified);
        assert!(entry.is_authoritative());
        assert!(entry.promote());
        assert_eq!(entry.state, ReflexionState::Archived);
        assert!(!entry.promote()); // terminal
    }

    #[test]
    fn reflexion_markdown_includes_root_cause() {
        let entry = ReflexionEntry {
            case_id: "c1".into(),
            state: ReflexionState::Provisional,
            new_patterns: vec!["p1".into()],
            applied_knowledge: Vec::new(),
            root_cause: Some("OOM".into()),
            improvements: vec!["check pool size earlier".into()],
            confidence: 0.8,
        };
        let md = entry.to_markdown();
        assert!(md.contains("# Reflexion: c1"));
        assert!(md.contains("OOM"));
        assert!(md.contains("check pool size earlier"));
    }
}
