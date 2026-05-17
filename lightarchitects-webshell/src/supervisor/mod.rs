//! Northstar supervision — tracks wave evaluations and signals drift proposals.
//!
//! The supervisor sits between the A2A event bus and the UI proposal system.
//! For each `WAVE_COMPLETE` event it receives, it calls [`evaluate_wave`] and
//! feeds the result into [`SupervisorState::record_evaluation`].  When
//! consecutive-drift count reaches `drift_threshold_waves`, the state machine
//! sets `proposal_pending = true` — the frontend renders a `ProposalCard`
//! allowing the operator to redirect the build (§Q check 5 + 6).

pub mod evaluation;

pub use evaluation::{EvaluationError, EvaluationStatus, NorthstarEvaluation, WaveContext};

/// Tunable parameters for one build's supervisor instance.
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// How many consecutive drifting waves trigger a proposal card.
    ///
    /// Default: 3.  Lowering this makes the supervisor more sensitive;
    /// raising it tolerates longer drift runs before interrupting.
    pub drift_threshold_waves: u32,
    /// Base URL of the Ollama instance used for evaluation (e.g. `http://localhost:11434`).
    ///
    /// When `None`, [`evaluate_wave`] returns a neutral stub without making
    /// any network requests.
    pub ollama_base: Option<String>,
    /// Ollama model name to use for evaluation calls.
    pub ollama_model: String,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            drift_threshold_waves: 3,
            ollama_base: None,
            ollama_model: "llama3".to_owned(),
        }
    }
}

/// Per-build supervisor state.
///
/// Tracks consecutive drift count and whether a proposal is already pending.
/// Cheap to clone — all fields are value types or `Option<String>`.
#[derive(Debug)]
pub struct SupervisorState {
    /// Northstar text to inject into evaluation prompts.
    ///
    /// `None` means the operator did not set a northstar at build-creation time;
    /// the supervisor still runs but evaluation returns a neutral stub.
    pub northstar_text: Option<String>,
    /// Runtime configuration.
    pub config: SupervisorConfig,
    /// Number of consecutive `Drifting` evaluations since the last reset.
    consecutive_drifts: u32,
    /// Whether a drift proposal is currently waiting for operator action.
    ///
    /// `true` suppresses further proposals until the operator acknowledges via
    /// [`SupervisorState::acknowledge_proposal`].
    pub proposal_pending: bool,
}

impl SupervisorState {
    /// Construct a new supervisor with default configuration.
    #[must_use]
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            northstar_text: None,
            config,
            consecutive_drifts: 0,
            proposal_pending: false,
        }
    }

    /// Set the northstar text for this build (builder-style).
    #[must_use]
    pub fn with_northstar(mut self, text: String) -> Self {
        self.northstar_text = Some(text);
        self
    }

    /// Record a wave evaluation and update drift state.
    ///
    /// Returns `true` when the consecutive-drift threshold is newly reached and
    /// a proposal card should be surfaced to the operator.  Returns `false` in
    /// all other cases (advancing/neutral wave, or a proposal is already pending).
    pub fn record_evaluation(&mut self, result: &NorthstarEvaluation) -> bool {
        match result.status {
            EvaluationStatus::Drifting => {
                self.consecutive_drifts += 1;
            }
            EvaluationStatus::Advancing | EvaluationStatus::Neutral => {
                self.consecutive_drifts = 0;
            }
        }

        let threshold_crossed =
            self.consecutive_drifts >= self.config.drift_threshold_waves && !self.proposal_pending;

        if threshold_crossed {
            self.proposal_pending = true;
        }
        threshold_crossed
    }

    /// Acknowledge the pending proposal and reset the drift counter.
    ///
    /// Call this after the operator selects an action from the proposal card.
    pub fn acknowledge_proposal(&mut self) {
        self.proposal_pending = false;
        self.consecutive_drifts = 0;
    }

    /// Current consecutive drift count (exposed for observability / tests).
    #[must_use]
    pub fn consecutive_drifts(&self) -> u32 {
        self.consecutive_drifts
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_eval_result(status: EvaluationStatus) -> NorthstarEvaluation {
        NorthstarEvaluation {
            status,
            confidence: 0.8,
            recommended_next: "test".to_owned(),
            wave_num: 0,
        }
    }

    #[test]
    fn consecutive_advancing_waves_do_not_trigger_proposal() {
        let mut state = SupervisorState::new(SupervisorConfig::default());
        for _ in 0..5 {
            let triggered = state.record_evaluation(&make_eval_result(EvaluationStatus::Advancing));
            assert!(!triggered);
        }
        assert_eq!(state.consecutive_drifts(), 0);
        assert!(!state.proposal_pending);
    }

    #[test]
    fn threshold_reached_triggers_proposal_once() {
        let config = SupervisorConfig {
            drift_threshold_waves: 3,
            ..Default::default()
        };
        let mut state = SupervisorState::new(config);

        assert!(!state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting)));
        assert!(!state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting)));
        // Third consecutive drift crosses threshold.
        let triggered = state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        assert!(triggered, "proposal should trigger at threshold");
        assert!(state.proposal_pending);

        // Fourth drift — proposal already pending, should NOT re-trigger.
        let re_triggered = state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        assert!(
            !re_triggered,
            "must not re-trigger while proposal is pending"
        );
    }

    #[test]
    fn advancing_wave_resets_drift_counter() {
        let mut state = SupervisorState::new(SupervisorConfig::default());
        state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        state.record_evaluation(&make_eval_result(EvaluationStatus::Advancing));

        assert_eq!(
            state.consecutive_drifts(),
            0,
            "drift counter must reset on advancing"
        );
    }

    #[test]
    fn acknowledge_proposal_resets_state() {
        let config = SupervisorConfig {
            drift_threshold_waves: 2,
            ..Default::default()
        };
        let mut state = SupervisorState::new(config);
        state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        assert!(state.proposal_pending);

        state.acknowledge_proposal();
        assert!(!state.proposal_pending);
        assert_eq!(state.consecutive_drifts(), 0);

        // After acknowledgement, should be able to trigger again.
        state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        state.record_evaluation(&make_eval_result(EvaluationStatus::Drifting));
        assert!(state.proposal_pending);
    }

    #[test]
    fn with_northstar_sets_text() {
        let state = SupervisorState::new(SupervisorConfig::default())
            .with_northstar("Ship E2E webshell".to_owned());
        assert_eq!(state.northstar_text.as_deref(), Some("Ship E2E webshell"));
    }
}
