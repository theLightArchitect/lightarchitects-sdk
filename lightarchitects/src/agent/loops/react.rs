//! `ReAct` (Reasoning + Acting) strategy — SDK port of QUANTUM agentic/`react.rs`.
//!
//! Implements the investigation loop: Scan → Sweep → Trace → Probe → Theorize → Verify → Close.
//! Each step the executor observes, reasons, and acts.
//!
//! Source: Yao et al. 2023, "`ReAct`: Synergizing Reasoning and Acting in Language Models"

use std::fmt::Write as _;

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Phase ─────────────────────────────────────────────────────────────────────

/// Investigation phase in the [`ReActStrategy`] lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReActPhase {
    /// Phase 1 — passive log scan, baseline collection.
    Scan,
    /// Phase 2 — active sweep of collected evidence.
    Sweep,
    /// Phase 3 — trace pattern matching across artifacts.
    Trace,
    /// Phase 4 — targeted probe (API queries, config dumps).
    Probe,
    /// Phase 5 — hypothesis generation from evidence.
    Theorize,
    /// Phase 6 — hypothesis verification.
    Verify,
    /// Phase 7 — close and record conclusions.
    Close,
}

impl ReActPhase {
    /// Advance to the next phase. [`ReActPhase::Close`] stays at `Close`.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Scan => Self::Sweep,
            Self::Sweep => Self::Trace,
            Self::Trace => Self::Probe,
            Self::Probe => Self::Theorize,
            Self::Theorize => Self::Verify,
            Self::Verify | Self::Close => Self::Close,
        }
    }
}

impl std::fmt::Display for ReActPhase {
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

// ── Step ──────────────────────────────────────────────────────────────────────

/// One completed step in the [`ReActStrategy`] investigation loop.
#[derive(Debug, Clone)]
pub struct ReActStep {
    /// What was observed or collected.
    pub observation: String,
    /// Chain-of-thought reasoning about the observation.
    pub thought: String,
    /// Action taken based on the reasoning.
    pub action: String,
    /// Result of the action, if available.
    pub result: Option<String>,
    /// Phase during which this step was taken (metadata).
    pub phase: ReActPhase,
}

// ── Prompt (state) ────────────────────────────────────────────────────────────

/// State threaded through each step of the [`ReActStrategy`] loop.
#[derive(Debug, Clone)]
pub struct ReActPrompt {
    /// Investigation query / problem statement.
    pub query: String,
    /// Additional context (prior knowledge, constraints).
    pub context: String,
    /// Steps accumulated across the investigation.
    pub steps: Vec<ReActStep>,
    /// Current phase.
    pub phase: ReActPhase,
    /// Maximum number of steps before a forced halt.
    pub max_steps: usize,
}

impl ReActPrompt {
    /// Start a new investigation at the `Scan` phase.
    #[must_use]
    pub fn new(query: impl Into<String>, max_steps: usize) -> Self {
        Self {
            query: query.into(),
            context: String::new(),
            steps: Vec::new(),
            phase: ReActPhase::Scan,
            max_steps,
        }
    }

    /// Returns `true` when the step budget is exhausted.
    #[must_use]
    pub fn exceeded_limit(&self) -> bool {
        self.steps.len() >= self.max_steps
    }

    /// Build a prompt string suitable for passing to an LLM.
    #[must_use]
    pub fn to_prompt_text(&self) -> String {
        let mut prompt = format!("Investigation: {}\nPhase: {}\n", self.query, self.phase);
        if !self.context.is_empty() {
            let _ = writeln!(prompt, "Context: {}", self.context);
        }
        for (i, step) in self.steps.iter().enumerate() {
            let _ = write!(
                prompt,
                "\nStep {} [{}]\nObservation: {}\nThought: {}\nAction: {}",
                i + 1,
                step.phase,
                step.observation,
                step.thought,
                step.action
            );
            prompt.push('\n');
            if let Some(ref result) = step.result {
                let _ = writeln!(prompt, "Result: {result}");
            }
        }
        prompt
    }
}

// ── Executor ──────────────────────────────────────────────────────────────────

/// Provider-agnostic executor for one [`ReActStrategy`] step.
#[async_trait]
pub trait ReActExecutor: Send + Sync + 'static {
    /// Execute one investigation step and return the result.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn step(&self, prompt: &ReActPrompt, ctx: &StepContext) -> Result<ReActStep, LoopError>;
}

// ── Strategy ─────────────────────────────────────────────────────────────────

/// `ReAct` investigation loop — drives a [`ReActExecutor`] through all 7 phases.
///
/// The loop halts when the phase reaches [`ReActPhase::Close`] or the step
/// budget is exceeded (whichever comes first).
pub struct ReActStrategy<E> {
    executor: E,
    name: &'static str,
}

impl<E: ReActExecutor> ReActStrategy<E> {
    /// Create a strategy wrapping the given executor.
    #[must_use]
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            name: "ReAct",
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
impl<E: ReActExecutor> Strategy for ReActStrategy<E> {
    type State = ReActPrompt;
    type Output = ReActPrompt;

    async fn step(
        &self,
        mut state: ReActPrompt,
        ctx: &StepContext,
    ) -> Result<Outcome<ReActPrompt, ReActPrompt>, LoopError> {
        if state.phase == ReActPhase::Close || state.exceeded_limit() {
            return Ok(Outcome::Halt(state));
        }
        let react_step = self.executor.step(&state, ctx).await?;
        state.steps.push(react_step);
        state.phase = state.phase.next();
        Ok(if state.phase == ReActPhase::Close {
            Outcome::Halt(state)
        } else {
            Outcome::Continue(state)
        })
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

    struct StubExecutor;

    #[async_trait::async_trait]
    impl ReActExecutor for StubExecutor {
        async fn step(
            &self,
            prompt: &ReActPrompt,
            _ctx: &StepContext,
        ) -> Result<ReActStep, LoopError> {
            Ok(ReActStep {
                observation: "observed".into(),
                thought: "thinking".into(),
                action: "acted".into(),
                result: Some("result".into()),
                phase: prompt.phase,
            })
        }
    }

    #[tokio::test]
    async fn react_advances_through_all_phases() {
        let runner = LoopRunner::new(ReActStrategy::new(StubExecutor), Budget::unlimited());
        let mut stream = runner.run(
            ReActPrompt::new("query", 100),
            ChainContext::default(),
            None,
        );

        let mut step_count = 0u32;
        let mut halted = false;
        while let Some(result) = stream.next().await {
            let step = result.unwrap();
            step_count += 1;
            if let Outcome::Halt(ref out) = step.outcome {
                assert_eq!(out.phase, ReActPhase::Close);
                assert_eq!(out.steps.len(), 6);
                halted = true;
            }
        }
        // 6 Continue + 1 final Halt = 7 stream items... actually no:
        // Verify→Close triggers Halt in the strategy.step(), so step 6 = Halt(Close).
        // Steps: Scan(1)→Sweep(C), Sweep(2)→Trace(C), Trace(3)→Probe(C),
        //        Probe(4)→Theorize(C), Theorize(5)→Verify(C), Verify(6)→Close(H)
        assert_eq!(
            step_count, 6,
            "expect 6 steps (final Verify→Close emits Halt)"
        );
        assert!(halted);
    }

    #[tokio::test]
    async fn react_halts_at_step_limit() {
        let runner = LoopRunner::new(ReActStrategy::new(StubExecutor), Budget::unlimited());
        let mut stream = runner.run(ReActPrompt::new("query", 2), ChainContext::default(), None);

        let mut step_count = 0u32;
        while let Some(result) = stream.next().await {
            result.unwrap();
            step_count += 1;
        }
        // Step 1: len=0, execute, len=1, Sweep → Continue
        // Step 2: len=1, execute, len=2, Trace → Continue
        // Step 3: len=2 >= max_steps=2, immediate Halt → emits Halt
        assert_eq!(step_count, 3);
    }

    #[test]
    fn react_phase_transitions_are_correct() {
        assert_eq!(ReActPhase::Scan.next(), ReActPhase::Sweep);
        assert_eq!(ReActPhase::Sweep.next(), ReActPhase::Trace);
        assert_eq!(ReActPhase::Verify.next(), ReActPhase::Close);
        assert_eq!(ReActPhase::Close.next(), ReActPhase::Close);
    }

    #[test]
    fn react_prompt_text_includes_all_steps() {
        let mut p = ReActPrompt::new("memory leak?", 10);
        p.context = "production system".into();
        p.steps.push(ReActStep {
            observation: "high mem".into(),
            thought: "possible leak".into(),
            action: "grep logs".into(),
            result: Some("OOM found".into()),
            phase: ReActPhase::Scan,
        });
        let text = p.to_prompt_text();
        assert!(text.contains("memory leak?"));
        assert!(text.contains("production system"));
        assert!(text.contains("high mem"));
        assert!(text.contains("OOM found"));
    }
}
