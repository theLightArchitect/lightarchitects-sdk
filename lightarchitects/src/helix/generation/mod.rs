//! Generation strategies for `Helix`-backed retrieval-augmented generation.
//!
//! This module exposes empirically-validated prompt policies and context
//! strategies derived from a 500-question study on `LongMemEval-S`
//! (2026-05-27, Sonnet 4.6 + 3-judge ensemble). Final score: 0.858/1.000.
//!
//! # Key types
//!
//! - [`QuestionIntent`] (re-exported from [`crate::helix::types`]) — classifies
//!   a user query into one of five intents.
//! - [`ContextStrategy`] — whether to use ranked retrieval or full conversation
//!   context for a given query.
//! - [`IntentClassifier`] — pluggable trait for intent classification.
//! - [`KeywordIntentClassifier`] — default keyword-based implementation.
//! - [`PromptPolicy`] — selects a prompt template for a given intent.
//! - The `policies` submodule exposes the verbatim `v3-winning` prompt strings
//!   that produced the 0.858 result.
//!
//! # Example
//!
//! ```ignore
//! use lightarchitects::helix::generation::{
//!     KeywordIntentClassifier, IntentClassifier, ContextStrategy,
//!     PromptPolicy, optimal_strategy_for_intent,
//! };
//!
//! let classifier = KeywordIntentClassifier;
//! let intent = classifier.classify("How many model kits did I buy?");
//! let strategy = optimal_strategy_for_intent(intent);
//! let policy = PromptPolicy::for_intent(intent);
//!
//! // strategy.is_full_context() ⇒ true (Counting → FullContext)
//! // policy.user_template() starts with "ANSWERING POLICY:\nThis question asks for a count..."
//! ```
//!
//! # Empirical results per intent
//!
//! | Intent       | Strategy             | Policy template                  | Sonnet 4.6 acc |
//! |--------------|----------------------|----------------------------------|---------------:|
//! | `Literal`    | `Ranked`             | `V3_LITERAL_POLICY`              |        0.94-0.97 |
//! | `Preference` | `Ranked`             | `V3_PREFERENCE_POLICY`           |        0.833 |
//! | `Temporal`   | `Ranked` + boost     | `V3_TEMPORAL_POLICY`             |        0.795 |
//! | `Counting`   | `FullContext`        | `V3_COUNTING_POLICY`             |        0.760 |
//! | `Abstention` | `Ranked`             | `V3_LITERAL_POLICY` with UNKNOWN |        0.933 |

pub mod classifier;
pub mod completer;
pub mod policies;
pub mod telemetry;

pub use crate::helix::types::QuestionIntent;
pub use classifier::{IntentClassifier, KeywordIntentClassifier};
pub use completer::{
    AnthropicCompleter, Completion, CompletionError, LlmCompleter, OllamaCompleter,
    OpenAICompatCompleter, OpenAIFlavor, model_class_from_name,
};
pub use policies::{
    PromptPolicy, V3_COUNTING_POLICY, V3_LITERAL_POLICY, V3_PREFERENCE_POLICY, V3_SYSTEM_BASE,
    V3_TEMPORAL_POLICY,
};
pub use telemetry::{PipelineRunCtx, PipelineSpan, SpanInstrumented};

/// Context-construction strategy for retrieval-augmented generation.
///
/// Composes with `crate::helix::soul_search::RetrievalMode`: when the strategy
/// is [`ContextStrategy::Ranked`], the caller should use a `RetrievalMode` to
/// select the top-k from hybrid retrieval. When the strategy is
/// [`ContextStrategy::FullContext`], the caller should bypass ranking and
/// supply every step in the helix.
///
/// # Empirical guidance
///
/// `FullContext` is **strictly better than `Ranked`** only for
/// [`QuestionIntent::Counting`]: +12.6pp on the 500-question `LongMemEval-S`
/// study (Sonnet 4.6, 2026-05-27). For all other intents, `Ranked` ties or
/// beats `FullContext` (e.g. `Preference` regressed -27pp with `FullContext`,
/// because dilution exceeded coverage).
///
/// # Model-class dependency
///
/// `FullContext` is **Sonnet-class capability dependent**. Cheap long-context
/// models with nominal 1M+ token windows do not necessarily exhibit
/// proportional long-context attention quality:
///
/// - Sonnet 4.6 on `Counting` + `FullContext`: 0.760
/// - Llama 4 Scout on the same task: **0.218** (-45.9pp gap)
///
/// Callers using a non-Sonnet-class model should prefer
/// [`optimal_strategy_for_intent_with_model_class`] over the unconditional
/// [`optimal_strategy_for_intent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextStrategy {
    /// Use hybrid retrieval to select top-k snippets. The caller should pick a
    /// `RetrievalMode` (or call `RetrievalMode::from_step_count`) for signal
    /// fusion weights.
    Ranked,
    /// Bypass retrieval; feed the entire helix (all steps) as context.
    /// Optimal only for `Counting` intent with Sonnet-class models. See type
    /// docs for the empirical model-class caveat.
    FullContext,
}

impl ContextStrategy {
    /// `true` when this strategy bypasses ranked retrieval.
    #[must_use]
    pub fn is_full_context(self) -> bool {
        matches!(self, Self::FullContext)
    }

    /// `true` when this strategy uses ranked retrieval.
    #[must_use]
    pub fn is_ranked(self) -> bool {
        matches!(self, Self::Ranked)
    }

    /// Static string for logging / span attributes.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ranked => "ranked",
            Self::FullContext => "full_context",
        }
    }
}

/// Coarse model-capability tier. Used to gate strategies whose effectiveness
/// depends on long-context attention quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelClass {
    /// Frontier long-context model (Sonnet 4.6+, GPT-5 class, Gemini 2.5 Pro).
    /// Safe for `FullContext` strategy.
    Frontier,
    /// Mid-tier model (Gemini 2.0 Flash, GPT-4.1 mini, Sonnet 4.5).
    /// `FullContext` may degrade; prefer `Ranked` for Counting if accuracy is critical.
    MidTier,
    /// Cheap long-context model (Llama 4 Scout, Qwen3.5-Flash, GPT-4.1-nano).
    /// `FullContext` empirically collapses on counting tasks; prefer `Ranked`.
    Cheap,
}

/// Return the empirically-optimal [`ContextStrategy`] for the given intent,
/// assuming a frontier model.
///
/// Use [`optimal_strategy_for_intent_with_model_class`] if you need
/// model-class-aware routing.
#[must_use]
pub fn optimal_strategy_for_intent(intent: QuestionIntent) -> ContextStrategy {
    match intent {
        QuestionIntent::Counting => ContextStrategy::FullContext,
        _ => ContextStrategy::Ranked,
    }
}

/// Return the empirically-optimal [`ContextStrategy`] for the given intent,
/// taking the model's capability tier into account.
///
/// `FullContext` is downgraded to `Ranked` for non-Frontier models on
/// `Counting` because cheap models exhibit attention collapse on long
/// inputs (-45.9pp gap measured 2026-05-27 between Sonnet 4.6 and Llama 4
/// Scout on the same Counting task).
#[must_use]
pub fn optimal_strategy_for_intent_with_model_class(
    intent: QuestionIntent,
    model_class: ModelClass,
) -> ContextStrategy {
    match (intent, model_class) {
        (QuestionIntent::Counting, ModelClass::Frontier) => ContextStrategy::FullContext,
        _ => ContextStrategy::Ranked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontier_counting_picks_full_context() {
        assert_eq!(
            optimal_strategy_for_intent(QuestionIntent::Counting),
            ContextStrategy::FullContext
        );
    }

    #[test]
    fn cheap_counting_falls_back_to_ranked() {
        assert_eq!(
            optimal_strategy_for_intent_with_model_class(
                QuestionIntent::Counting,
                ModelClass::Cheap
            ),
            ContextStrategy::Ranked
        );
    }

    #[test]
    fn non_counting_always_ranked() {
        for intent in [
            QuestionIntent::Literal,
            QuestionIntent::Preference,
            QuestionIntent::Temporal,
            QuestionIntent::Abstention,
        ] {
            assert_eq!(optimal_strategy_for_intent(intent), ContextStrategy::Ranked);
            assert_eq!(
                optimal_strategy_for_intent_with_model_class(intent, ModelClass::Frontier),
                ContextStrategy::Ranked
            );
        }
    }

    #[test]
    fn strategy_string_labels_stable() {
        assert_eq!(ContextStrategy::Ranked.as_str(), "ranked");
        assert_eq!(ContextStrategy::FullContext.as_str(), "full_context");
    }
}
