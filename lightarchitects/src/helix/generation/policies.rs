//! Verbatim `v3-winning` prompt policies from the `LongMemEval-S` 2026-05-27
//! study.
//!
//! Each `POLICY` constant is the `ANSWERING POLICY` block appended to the user
//! prompt for a given [`QuestionIntent`]. The shared system prompt
//! [`V3_SYSTEM_BASE`] is the common preamble. Together they produced the 0.858
//! 500-question result against Sonnet 4.6 in the canonical study.
//!
//! These strings are also embedded verbatim in `soul-bench/src/main.rs` under
//! `generate_answer_v3_winning` — both source-of-truth copies are kept in sync
//! intentionally because the strings ARE the experimental contribution.

use crate::helix::types::QuestionIntent;

/// Universal system prompt for all `v3-winning` generation calls. Defines the
/// snippet format and three universal grounding constraints.
pub const V3_SYSTEM_BASE: &str = "\
You answer questions about a user's conversation history using ONLY the provided dated snippets.

Each snippet is prefixed with: [N] [step_date] role_and_content
The \"reference now\" date is the timestamp the user asked the question.

UNIVERSAL CONSTRAINTS:
- Ground every claim in the snippets. Do not invent facts.
- If two snippets give different values for the SAME fact, prefer the snippet with the LATER step_date — the user's state has updated.
- Never fabricate items, dates, or qualifiers that aren't in any snippet.";

/// Policy block for [`QuestionIntent::Literal`] and [`QuestionIntent::Abstention`].
///
/// Includes abstention-aware UNKNOWN guidance — say `UNKNOWN — does not mention X.
/// It does mention [related Y]` when the closest match is a different entity.
/// This single instruction took the abstention subset from a random baseline to
/// 28/30 = 0.933 in the canonical study.
pub const V3_LITERAL_POLICY: &str = "\
ANSWERING POLICY:
- Give the most specific exact answer present in the snippets. INCLUDE meaningful qualifiers \
  (\"marketing specialist at a small startup\", not just \"marketing specialist\").
- If a snippet directly mentions the entity in the question, give the relevant fact — even if \
  the wording differs. Indirect evidence counts.
- Refuse with \"UNKNOWN — the conversation does not mention [topic]\" ONLY if no snippet contains \
  the entity at all.
- If you find a CLOSELY-RELATED but DIFFERENT entity (e.g. question asks about hamster but only \
  cat is mentioned), say: \"UNKNOWN — the conversation does not mention [exact thing asked]. It \
  does mention [related thing actually present].\"

Final answer:";

/// Policy block for [`QuestionIntent::Preference`].
///
/// Reframes the task as "describe the user's stored preference" rather than
/// "recommend something from your knowledge". Took preference accuracy from
/// the universal-prompt baseline to 0.833 (+43pp) in the canonical study.
pub const V3_PREFERENCE_POLICY: &str = "\
ANSWERING POLICY:
This question asks you to SUGGEST or RECOMMEND something. The expected answer is NOT a new \
recommendation from your own knowledge — it is a description of what THE USER would prefer \
based on their documented preferences in the snippets.

- Scan the snippets for the user's preferences, habits, past choices, and stated likes/dislikes \
  RELATED to the question topic.
- Express the answer as: what the user would prefer / what suggestions would suit them, drawing \
  strictly from the snippets.
- Do NOT respond \"UNKNOWN\" unless the snippets contain ZERO preference signals on or near the \
  topic.

Final answer (describe the user's preference, not a fresh recommendation):";

/// Policy block for [`QuestionIntent::Temporal`].
///
/// Forces explicit date arithmetic rather than refusal. Combined with the
/// soul-bench temporal-proximity boost, took temporal accuracy from baseline
/// to 0.795 (+42pp) in the canonical study.
pub const V3_TEMPORAL_POLICY: &str = "\
ANSWERING POLICY:
This question involves date arithmetic or temporal ordering. You CAN and MUST compute date math \
from the snippet's step_date values and the reference_now date.

- \"How many days/weeks/months ago\" → identify the relevant snippet's step_date, compute \
  (reference_now − step_date) in the requested unit. Give the integer answer. Do NOT refuse.
- \"Which happened first, A or B?\" → compare the step_dates of the snippets mentioning A and B. \
  The one with the EARLIER step_date happened first.
- \"What is the order of N events?\" → sort the relevant snippets by step_date ascending, list \
  in chronological order.
- Off-by-one: when computing \"N days ago\", use whole days between dates (no inclusive/exclusive \
  ambiguity unless gold specifies).

Final answer (compute the value, do not refuse):";

/// Policy block for [`QuestionIntent::Counting`].
///
/// Forces explicit enumeration, deduplication, and strict criteria application
/// before committing to a count. Optimal context strategy for this policy is
/// `ContextStrategy::FullContext` (not top-k retrieval) — see the module
/// docs for the empirical justification.
pub const V3_COUNTING_POLICY: &str = "\
ANSWERING POLICY:
This question asks for a count or total (\"how many\", \"how much\", \"total\"). The snippets \
may not contain ALL mentions — you have the top-k most relevant.

- First, list each distinct mention with its snippet number (\"[1] X, [3] Y, [5] Z\").
- Then compute the count or total.
- For counts: if you see N distinct items, the answer is at least N. If the question implies \
  completeness (\"How many total X?\"), commit to your visible count — do not refuse just because \
  you might be missing some.
- For sums: add up the visible amounts; if the question says \"total\" and snippets show partial \
  totals, sum what's visible.

Final answer (number only on the last line, after listing):";

/// Wrapper that selects the right policy string for a given intent.
#[derive(Debug, Clone, Copy)]
pub struct PromptPolicy {
    intent: QuestionIntent,
}

impl PromptPolicy {
    /// Return the canonical [`PromptPolicy`] for the given intent.
    #[must_use]
    pub fn for_intent(intent: QuestionIntent) -> Self {
        Self { intent }
    }

    /// Return the `v3-winning` policy block (the `ANSWERING POLICY:` body)
    /// for the wrapped intent.
    #[must_use]
    pub fn user_template(self) -> &'static str {
        match self.intent {
            QuestionIntent::Literal | QuestionIntent::Abstention => V3_LITERAL_POLICY,
            QuestionIntent::Preference => V3_PREFERENCE_POLICY,
            QuestionIntent::Temporal => V3_TEMPORAL_POLICY,
            QuestionIntent::Counting => V3_COUNTING_POLICY,
        }
    }

    /// The shared system prompt — same for every intent.
    #[must_use]
    pub fn system_prompt(self) -> &'static str {
        V3_SYSTEM_BASE
    }

    /// The intent this policy was constructed for.
    #[must_use]
    pub fn intent(self) -> QuestionIntent {
        self.intent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_and_abstention_share_policy() {
        let lit = PromptPolicy::for_intent(QuestionIntent::Literal);
        let abs = PromptPolicy::for_intent(QuestionIntent::Abstention);
        assert_eq!(lit.user_template(), abs.user_template());
    }

    #[test]
    fn all_policies_are_distinct_strings_otherwise() {
        let lit = PromptPolicy::for_intent(QuestionIntent::Literal).user_template();
        let pref = PromptPolicy::for_intent(QuestionIntent::Preference).user_template();
        let temp = PromptPolicy::for_intent(QuestionIntent::Temporal).user_template();
        let count = PromptPolicy::for_intent(QuestionIntent::Counting).user_template();
        assert_ne!(lit, pref);
        assert_ne!(lit, temp);
        assert_ne!(lit, count);
        assert_ne!(pref, temp);
        assert_ne!(pref, count);
        assert_ne!(temp, count);
    }

    #[test]
    fn system_prompt_invariant_across_intents() {
        for intent in [
            QuestionIntent::Literal,
            QuestionIntent::Preference,
            QuestionIntent::Temporal,
            QuestionIntent::Counting,
            QuestionIntent::Abstention,
        ] {
            assert_eq!(
                PromptPolicy::for_intent(intent).system_prompt(),
                V3_SYSTEM_BASE
            );
        }
    }

    #[test]
    fn abstention_policy_contains_unknown_with_related() {
        let policy = PromptPolicy::for_intent(QuestionIntent::Abstention).user_template();
        assert!(policy.contains("does mention [related"));
    }
}
