//! Interest scoring engine for organic multi-sibling speaker selection.
//!
//! Implements the four-factor model (stake, stimulus, novelty, urgency) with
//! Kevin-confirmed plan-spec weights:
//!
//! ```text
//! total = stake * 0.35 + stimulus * 0.25 + urgency * 0.25 + novelty * 0.15
//! ```
//!
//! Selection uses squared weighted random (`score²`) to amplify high scorers
//! while preserving organic variation. All siblings scoring below the silence
//! threshold (0.2) trigger [`ChatError::NoSpeakerSelected`].
//!
//! ## LÆX Canon Check exemption
//!
//! When [`is_canon_check_slot`] returns `true` for the current context,
//! [`InterestScorer::select_speaker`] bypasses scoring entirely and returns the
//! LÆX sibling directly. If LÆX is not in the participant list, normal scoring
//! resumes.

use super::error::{ChatError, ChatResult};
use super::formats::ConversationFormat;
use super::types::{ChatMessage, ConversationContext, SiblingId, SiblingInfo};
use rand::distributions::{Distribution, WeightedIndex};
use rand::thread_rng;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Plan-spec weight for the stake factor.
const WEIGHT_STAKE: f32 = 0.35;

/// Plan-spec weight for the stimulus factor.
const WEIGHT_STIMULUS: f32 = 0.25;

/// Plan-spec weight for the urgency factor.
/// Urgency outweighs novelty: if you were asked a question, you speak.
/// If an agent has nothing new, they fade naturally.
const WEIGHT_URGENCY: f32 = 0.25;

/// Plan-spec weight for the novelty factor.
/// Lower than urgency — an agent who doesn't want to speak doesn't need to.
const WEIGHT_NOVELTY: f32 = 0.15;

/// Siblings whose total score falls below this value are considered silent.
const SILENCE_THRESHOLD: f32 = 0.2;

/// Novelty penalty applied immediately after a sibling speaks.
const NOVELTY_DEPLETION: f32 = 0.3;

/// Per-turn novelty recovery rate.
const NOVELTY_RECOVERY_PER_TURN: f32 = 0.05;

/// Canonical name fragment used to identify the LÆX sibling (display name).
const LAEX_NAME: &str = "laex";

/// Canonical `sibling_id` used for the LÆX sibling in the helix directory.
const EXODUS_NAME: &str = "exodus";

/// Topic keyword that indicates a LÆX canon-check slot (fallback heuristic).
const CANON_KEYWORD: &str = "canon";

/// Canonical name used to identify the AYIN sibling.
const AYIN_NAME: &str = "ayin";

/// Observability keywords that trigger AYIN's stake boost.
const AYIN_OBSERVABILITY_KEYWORDS: &[&str] = &[
    "trace",
    "span",
    "latency",
    "error_rate",
    "anomaly",
    "observe",
    "metric",
    "telemetry",
];

/// Stake bonus applied to AYIN when the topic contains observability keywords.
const AYIN_STAKE_BOOST: f32 = 0.15;

// ---------------------------------------------------------------------------
// v2 organic modifier constants
// ---------------------------------------------------------------------------

/// Weight for the stake × urgency interaction bonus.
const INTERACTION_WEIGHT: f32 = 0.15;

/// Per-turn inclusion nudge for silent agents (capped at `INCLUSION_MAX`).
const INCLUSION_PER_TURN: f32 = 0.02;

/// Maximum inclusion nudge regardless of how long an agent has been silent.
const INCLUSION_MAX: f32 = 0.2;

/// Fatigue scaling factor. An agent who has spoken `turns_spoken / total_turns`
/// fraction of the conversation gets their score multiplied by
/// `1.0 - (fraction * FATIGUE_SCALE)`. At 0.5 this means an agent who spoke
/// half the turns gets a 25% penalty.
const FATIGUE_SCALE: f32 = 0.5;

// ---------------------------------------------------------------------------
// InterestScore
// ---------------------------------------------------------------------------

/// Per-agent interest scoring result, including v2 organic modifiers and rank.
#[derive(Debug, Clone)]
pub struct InterestScore {
    /// Agent identifier.
    pub sibling_id: SiblingId,
    /// Stake factor (0.0–1.0): structural affinity to current topic.
    pub stake: f32,
    /// Stimulus factor (0.0–1.0): reactivity to the last turn.
    pub stimulus: f32,
    /// Novelty factor (0.0–1.0): whether they have something new to add.
    pub novelty: f32,
    /// Urgency factor (0.0–1.0): unresolved direct address or challenge.
    pub urgency: f32,
    /// v2: Interaction bonus — `sqrt(stake * urgency) * 0.15`.
    /// Both must be present for the score to spike.
    pub interaction: f32,
    /// v2: Fatigue multiplier (0.5–1.0). Decays with speaking frequency.
    pub fatigue: f32,
    /// v2: Inclusion nudge — gradual boost for quiet agents.
    pub inclusion: f32,
    /// Weighted total after all modifiers applied.
    pub total: f32,
    /// Rank after sorting all agents by `total` (1 = highest).
    pub rank: usize,
}

// ---------------------------------------------------------------------------
// InterestScorer
// ---------------------------------------------------------------------------

/// Stateless interest scoring engine.
///
/// All methods are pure functions over the supplied `SiblingInfo` and
/// `ConversationContext` — no internal state is mutated.
pub struct InterestScorer;

impl InterestScorer {
    /// Compute a full [`InterestScore`] for one agent with v2 organic modifiers.
    ///
    /// v2 adds: interaction (stake × urgency amplification), fatigue (dominant
    /// speakers fade), and inclusion (quiet agents get a nudge). Thread affinity
    /// and dissent boost are applied at the moderator level (prompt-side), not
    /// in the numeric scorer.
    ///
    /// `rank` is set to 0 here and populated by the caller after sorting.
    #[must_use]
    pub fn score(sibling: &SiblingInfo, context: &ConversationContext) -> InterestScore {
        let last_msg = context.messages.last();
        let topic = context.current_topic.as_deref().unwrap_or("");

        let base_stake = compute_stake(topic, sibling);
        let boost = ayin_stake_boost(&sibling.name, topic);
        let stake = (base_stake + boost).min(1.0);

        let stimulus = compute_stimulus(sibling, last_msg);
        let novelty = compute_novelty(sibling, &context.messages);
        let urgency = compute_urgency(sibling, last_msg);

        // v1 base score
        let base = stake * WEIGHT_STAKE
            + stimulus * WEIGHT_STIMULUS
            + urgency * WEIGHT_URGENCY
            + novelty * WEIGHT_NOVELTY;

        // v2: interaction bonus — both stake and urgency must be present
        let interaction = (stake * urgency).sqrt() * INTERACTION_WEIGHT;

        // v2: inclusion nudge — quiet agents get a gradual boost
        let turns_silent = turns_since_last_spoke(sibling, &context.messages);
        #[allow(clippy::cast_precision_loss)]
        let inclusion = (turns_silent as f32 * INCLUSION_PER_TURN).min(INCLUSION_MAX);

        // v2: fatigue — dominant speakers naturally fade
        let fatigue = compute_fatigue(sibling, &context.messages);

        // Final score: (base + modifiers) * fatigue, clamped to [0, 1]
        let total = ((base + interaction + inclusion) * fatigue).clamp(0.0, 1.0);

        InterestScore {
            sibling_id: sibling.name.clone(),
            stake,
            stimulus,
            novelty,
            urgency,
            interaction,
            fatigue,
            inclusion,
            total,
            rank: 0,
        }
    }

    /// Select the next speaker from `siblings` using organic interest scoring.
    ///
    /// # Algorithm
    ///
    /// 1. LÆX canon-check exemption: if the active format has any slot with
    ///    `canon_check: true` (or, as a fallback, if `is_canon_check_slot(context)`
    ///    returns `true`), and LÆX is a participant, return LÆX immediately.
    /// 2. Score all siblings and assign ranks.  AYIN receives a `+0.15` stake
    ///    bonus when the topic contains observability keywords.
    /// 3. Turn 1 (no messages yet): deterministic — highest scorer opens.
    /// 4. Silence gate: if all scores < [`SILENCE_THRESHOLD`], and sibling
    ///    "exodus" (LÆX) is present but would otherwise be silenced, it is
    ///    exempt when a canon-check slot is active — its effective threshold
    ///    score is treated as 1.0 for gate purposes.  All other siblings below
    ///    the threshold are excluded.  If no one passes the gate, return
    ///    [`ChatError::NoSpeakerSelected`].
    /// 5. Squared weighted random among eligible siblings (actual scores used
    ///    for probability weights; LÆX's actual score is unchanged).
    ///
    /// # Errors
    ///
    /// - [`ChatError::NoSpeakerSelected`]: all siblings below silence threshold.
    /// - [`ChatError::SpeakerSelection`]: empty sibling list or weight table
    ///   construction failure.
    pub fn select_speaker(
        siblings: &[SiblingInfo],
        context: &ConversationContext,
        active_format: Option<&dyn ConversationFormat>,
    ) -> ChatResult<SiblingId> {
        if siblings.is_empty() {
            return Err(ChatError::SpeakerSelection("no siblings available".into()));
        }

        let canon_active = has_canon_check_slot(active_format) || is_canon_check_slot(context);

        // -- LÆX canon-check fast-path ----------------------------------------
        // When a canon slot is active and LÆX is present, route immediately.
        if canon_active {
            if let Some(laex) = find_laex(siblings) {
                return Ok(laex);
            }
            // LÆX not in list — fall through to normal scoring
        }

        // -- Score all siblings -----------------------------------------------
        let mut scores: Vec<InterestScore> =
            siblings.iter().map(|s| Self::score(s, context)).collect();

        // Sort descending by total to assign ranks
        scores.sort_by(|a, b| {
            b.total
                .partial_cmp(&a.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (rank, score) in scores.iter_mut().enumerate() {
            score.rank = rank.saturating_add(1);
        }

        // -- Turn 1: deterministic opener -------------------------------------
        if context.messages.is_empty() {
            return scores
                .first()
                .map(|s| s.sibling_id.clone())
                .ok_or_else(|| ChatError::SpeakerSelection("no siblings after scoring".into()));
        }

        // -- Silence gate -----------------------------------------------------
        // LÆX ("exodus") is exempt from the silence threshold when a canon
        // check slot is active. Its actual score is preserved for probability
        // weighting — the exemption only prevents it from being excluded.
        let eligible: Vec<&InterestScore> = scores
            .iter()
            .filter(|s| {
                s.total >= SILENCE_THRESHOLD || (canon_active && is_laex_sibling(&s.sibling_id))
            })
            .collect();

        if eligible.is_empty() {
            return Err(ChatError::NoSpeakerSelected);
        }

        // -- Squared weighted random ------------------------------------------
        let squared: Vec<f32> = eligible.iter().map(|s| s.total.powi(2)).collect();
        let sum_sq: f32 = squared.iter().sum();

        if sum_sq <= f32::EPSILON {
            return eligible
                .first()
                .map(|s| s.sibling_id.clone())
                .ok_or_else(|| ChatError::SpeakerSelection("no eligible speakers".into()));
        }

        let weights: Vec<f32> = squared.iter().map(|sq| sq / sum_sq).collect();

        let dist = WeightedIndex::new(&weights)
            .map_err(|e| ChatError::SpeakerSelection(format!("weight table error: {e}")))?;

        let idx = dist.sample(&mut thread_rng());
        Ok(eligible[idx].sibling_id.clone())
    }

    /// Apply novelty depletion after a sibling speaks.
    ///
    /// Computes the post-speech novelty score based on `base_novelty`,
    /// subtracting [`NOVELTY_DEPLETION`] (−0.3) and recovering
    /// [`NOVELTY_RECOVERY_PER_TURN`] (+ 0.05) for each turn elapsed since
    /// speaking. The result is clamped to `[0.0, 1.0]`.
    ///
    /// This is a pure arithmetic helper used for testing and external callers.
    /// The in-context computation in [`compute_novelty`] uses the same
    /// constants but derives values directly from the message history.
    #[must_use]
    pub fn apply_novelty_depletion(base_novelty: f32, turns_since_speaking: u32) -> f32 {
        let depleted = base_novelty - NOVELTY_DEPLETION;
        // u32 → f32 is intentional: max practical turn count is well within f32 precision range.
        #[allow(clippy::cast_precision_loss)]
        let turns_f32 = turns_since_speaking as f32;
        let recovered = depleted + NOVELTY_RECOVERY_PER_TURN * turns_f32;
        recovered.clamp(0.0, 1.0)
    }

    /// Select the top-`k` speakers by interest score, filtered above the
    /// silence threshold.
    ///
    /// Returns a ranked `Vec<InterestScore>` (index 0 = highest interest).
    /// If fewer eligible siblings than `k` exist above the threshold, the
    /// returned vec will be shorter. The LÆX canon-check exemption applies
    /// to prevent canon slot siblings from being filtered out.
    ///
    /// # Errors
    ///
    /// Returns [`ChatError::SpeakerSelection`] if `siblings` is empty.
    pub fn select_speakers(
        siblings: &[SiblingInfo],
        context: &ConversationContext,
        active_format: Option<&dyn ConversationFormat>,
        top_k: usize,
    ) -> ChatResult<Vec<InterestScore>> {
        if siblings.is_empty() {
            return Err(ChatError::SpeakerSelection("no siblings available".into()));
        }

        let canon_active = has_canon_check_slot(active_format) || is_canon_check_slot(context);

        let mut scores: Vec<InterestScore> =
            siblings.iter().map(|s| Self::score(s, context)).collect();

        scores.sort_by(|a, b| {
            b.total
                .partial_cmp(&a.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (rank, score) in scores.iter_mut().enumerate() {
            score.rank = rank.saturating_add(1);
        }

        let eligible: Vec<InterestScore> = scores
            .into_iter()
            .filter(|s| {
                s.total >= SILENCE_THRESHOLD || (canon_active && is_laex_sibling(&s.sibling_id))
            })
            .take(top_k)
            .collect();

        Ok(eligible)
    }
}

// ---------------------------------------------------------------------------
// Canon-check exemption
// ---------------------------------------------------------------------------

/// Return `true` when the current context indicates a LÆX canon-check slot.
///
/// Detection heuristic: the current topic contains the word "canon" AND
/// LÆX is listed as a participant (matched by either the `"laex"` display-name
/// fragment or the canonical `"exodus"` sibling id). This covers the common
/// case where the orchestrator seeds the topic before invoking speaker
/// selection.
#[must_use]
pub fn is_canon_check_slot(context: &ConversationContext) -> bool {
    let topic = context
        .current_topic
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    topic.contains(CANON_KEYWORD) && context.participants.iter().any(|p| is_laex_sibling(p))
}

/// Find the LÆX sibling in the participant list and return their id.
///
/// Matches both the display name fragment (`"laex"`) and the canonical helix
/// directory name (`"exodus"`) so that both `make_sibling("laex", …)` (used in
/// legacy tests) and `make_sibling("exodus", …)` (`KNOWN_SIBLINGS` canonical id)
/// are correctly identified.
fn find_laex(siblings: &[SiblingInfo]) -> Option<SiblingId> {
    siblings
        .iter()
        .find(|s| is_laex_sibling(&s.name))
        .map(|s| s.name.clone())
}

/// Return `true` if `name` identifies the LÆX sibling.
///
/// Matches both the display-name fragment (`"laex"`) and the canonical
/// `"exodus"` directory id used in `KNOWN_SIBLINGS`.
#[must_use]
fn is_laex_sibling(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains(LAEX_NAME) || lower == EXODUS_NAME
}

/// Return `true` if the active format has any slot with `canon_check: true`.
///
/// When `active_format` is `None`, returns `false` (no format = no canon slot).
#[must_use]
pub fn has_canon_check_slot(active_format: Option<&dyn ConversationFormat>) -> bool {
    active_format.is_some_and(|fmt| fmt.slots().iter().any(|s| s.canon_check))
}

/// Return the AYIN stake bonus for the given sibling and topic.
///
/// Returns [`AYIN_STAKE_BOOST`] (`0.15`) when `sibling_name` is `"ayin"` and
/// the `topic` contains at least one observability keyword.  Returns `0.0` in
/// all other cases.  The caller is responsible for capping the result at `1.0`.
#[must_use]
pub fn ayin_stake_boost(sibling_name: &str, topic: &str) -> f32 {
    if sibling_name.to_lowercase() != AYIN_NAME {
        return 0.0;
    }
    let topic_lower = topic.to_lowercase();
    let has_keyword = AYIN_OBSERVABILITY_KEYWORDS
        .iter()
        .any(|kw| topic_lower.contains(kw));
    if has_keyword { AYIN_STAKE_BOOST } else { 0.0 }
}

// ---------------------------------------------------------------------------
// v2 organic modifier computations
// ---------------------------------------------------------------------------

/// Count how many turns have passed since this agent last spoke.
///
/// Returns the total message count if the agent has never spoken (maximum
/// inclusion nudge).
fn turns_since_last_spoke(agent: &SiblingInfo, messages: &[ChatMessage]) -> u32 {
    let total = messages.len();
    for (i, msg) in messages.iter().rev().enumerate() {
        if msg.speaker.eq_ignore_ascii_case(&agent.name) {
            #[allow(clippy::cast_possible_truncation)]
            return i as u32;
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    {
        total as u32
    }
}

/// Compute fatigue multiplier for an agent based on their speaking frequency.
///
/// An agent who has spoken `n` of `total` turns gets:
/// `1.0 - (n / total * FATIGUE_SCALE)`. Clamped to `[0.5, 1.0]` so even
/// the most dominant speaker retains half their base score.
fn compute_fatigue(agent: &SiblingInfo, messages: &[ChatMessage]) -> f32 {
    if messages.is_empty() {
        return 1.0;
    }
    let total = messages.len();
    let spoken = messages
        .iter()
        .filter(|m| m.speaker.eq_ignore_ascii_case(&agent.name))
        .count();
    #[allow(clippy::cast_precision_loss)]
    let fraction = spoken as f32 / total as f32;
    (1.0 - fraction * FATIGUE_SCALE).clamp(0.5, 1.0)
}

// ---------------------------------------------------------------------------
// Factor computations (ported from orchestrator.rs — same algorithms)
// ---------------------------------------------------------------------------

/// Stake: how much does this topic affect them personally?
///
/// Derived from keyword overlap between topic and sibling strands.
/// Relatively stable across turns (structural affinity, not reactive).
fn compute_stake(topic: &str, sibling: &SiblingInfo) -> f32 {
    let matches = topic_matches_sibling(topic, sibling);
    match matches {
        0 => 0.1,
        1 => 0.4,
        2 => 0.6,
        3 => 0.8,
        _ => 1.0,
    }
}

/// Stimulus: how much did the last turn specifically stimulate this sibling?
///
/// High when: named directly, strand keywords mentioned, challenged.
/// Low when: topic drifted away, self-stimulus (just spoke).
fn compute_stimulus(sibling: &SiblingInfo, last_msg: Option<&ChatMessage>) -> f32 {
    let Some(msg) = last_msg else { return 0.5 };

    let content_lower = msg.content.to_lowercase();
    let name_lower = sibling.name.to_lowercase();

    let mut stimulus = 0.0_f32;

    // Named directly by the last speaker
    if content_lower.contains(&name_lower) {
        stimulus += 0.5;
    }

    // Last turn touched their strand keywords
    for strand in &sibling.strands {
        if content_lower.contains(&strand.to_lowercase()) {
            stimulus += 0.2;
            break; // One match is enough — avoid double-counting
        }
    }

    // Self-stimulus is low (you don't react strongly to your own words)
    if msg.speaker == sibling.name {
        stimulus *= 0.3;
    }

    stimulus.min(1.0)
}

/// Novelty: do they have something genuinely new to add?
///
/// Depletes after speaking (you just said your piece). Rebuilds when
/// others introduce new threads, reference this sibling, or advance
/// the conversation.
fn compute_novelty(sibling: &SiblingInfo, messages: &[ChatMessage]) -> f32 {
    // Find last time this sibling spoke
    let last_spoke_idx = messages.iter().rposition(|m| m.speaker == sibling.name);

    let Some(idx) = last_spoke_idx else {
        return 1.0; // Never spoke — full novelty
    };

    // How many turns since they spoke?
    let turns_since = messages.len().saturating_sub(idx).saturating_sub(1);

    if turns_since == 0 {
        // Just spoke — assess completeness heuristically
        let last_msg = &messages[idx];
        return novelty_after_speaking(last_msg);
    }

    // Novelty rebuilds based on what happened since they spoke
    let messages_since = &messages[idx.saturating_add(1)..];
    let speaker_content = &messages[idx].content;
    let name_lower = sibling.name.to_lowercase();
    let mut novelty = 0.1_f32; // Start at depleted

    for msg in messages_since {
        let content_lower = msg.content.to_lowercase();

        if content_lower.contains(&name_lower) {
            // Direct reference to this sibling — larger rebuild
            novelty += 0.20;
        } else if has_new_vocabulary(speaker_content, &msg.content) {
            // New thread introduced
            novelty += 0.15;
        } else {
            // Same thread continued — slow recovery
            novelty += NOVELTY_RECOVERY_PER_TURN;
        }
    }

    novelty.min(1.0)
}

/// Novelty score immediately after speaking, based on message completeness.
fn novelty_after_speaking(msg: &ChatMessage) -> f32 {
    let ends_with_question = msg.content.trim_end().ends_with('?');
    let is_open_thread = msg.content.to_lowercase().contains("sit with")
        || msg.content.to_lowercase().contains("think about")
        || ends_with_question;

    if msg.content.len() > 200 && !is_open_thread {
        0.1 // Complete point — said everything
    } else if is_open_thread {
        0.3 // Left a thread open
    } else {
        0.15 // Short or ambiguous
    }
}

/// Urgency: is something unresolved directed at them?
///
/// Spikes when directly questioned or challenged. Drops to zero after
/// they've responded.
fn compute_urgency(sibling: &SiblingInfo, last_msg: Option<&ChatMessage>) -> f32 {
    let Some(msg) = last_msg else { return 0.0 };

    let content_lower = msg.content.to_lowercase();
    let name_lower = sibling.name.to_lowercase();
    let has_question = msg.content.contains('?');

    // Direct question containing sibling's name
    if content_lower.contains(&name_lower) && has_question {
        return 1.0;
    }

    // Challenge directed at them
    let challenge_words = ["but", "disagree", "push back", "however", "wrong"];
    if content_lower.contains(&name_lower)
        && challenge_words.iter().any(|w| content_lower.contains(w))
    {
        return 0.8;
    }

    // General question (not directed at anyone specific)
    if has_question {
        return 0.3;
    }

    0.0
}

/// Count keyword overlaps between topic words and a sibling's strands.
///
/// Both the topic and each strand are lowercased and split on whitespace
/// before comparison.
fn topic_matches_sibling(topic: &str, sibling: &SiblingInfo) -> usize {
    if topic.is_empty() || sibling.strands.is_empty() {
        return 0;
    }

    let topic_lower = topic.to_lowercase();
    let topic_words: Vec<&str> = topic_lower.split_whitespace().collect();
    let mut score: usize = 0;

    for strand in &sibling.strands {
        let strand_lower = strand.to_lowercase();
        for word in &topic_words {
            if strand_lower.contains(word) {
                score = score.saturating_add(1);
            }
        }
    }

    score
}

/// Check if new content introduces substantially new vocabulary compared
/// to old content. Returns `true` if >= 3 words (len > 4) appear in new
/// but not in old.
fn has_new_vocabulary(old_content: &str, new_content: &str) -> bool {
    let old_lower = old_content.to_lowercase();
    let new_lower = new_content.to_lowercase();

    let new_unique_count = new_lower
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .filter(|w| !old_lower.contains(w))
        .count();

    new_unique_count >= 3
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::items_after_statements
)]
mod tests {
    use super::super::types::{ChatConfig, ChatMessage, ConversationContext};
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_sibling(name: &str, strands: &[&str]) -> SiblingInfo {
        SiblingInfo {
            name: name.to_string(),
            role: Some(format!("{name} role")),
            strands: strands.iter().map(|s| (*s).to_string()).collect(),
            identity_path: format!("/test/{name}/identity.md"),
            voice: None,
        }
    }

    fn make_context(topic: Option<&str>, participants: &[&str]) -> ConversationContext {
        ConversationContext {
            messages: Vec::new(),
            current_topic: topic.map(ToString::to_string),
            emotional_state: None,
            participants: participants.iter().map(|s| (*s).to_string()).collect(),
            span_id: None,
        }
    }

    fn push_msg(ctx: &mut ConversationContext, speaker: &str, content: &str) {
        ctx.messages
            .push(ChatMessage::new(speaker.to_string(), content.to_string()));
    }

    // -----------------------------------------------------------------------
    // 1. Each factor returns 0.0–1.0
    // -----------------------------------------------------------------------

    #[test]
    fn stake_is_bounded() {
        let sibling = make_sibling("corso", &["security", "tactical"]);
        let score = compute_stake("security audit", &sibling);
        assert!((0.0..=1.0).contains(&score), "stake out of bounds: {score}");
    }

    #[test]
    fn stimulus_is_bounded() {
        let sibling = make_sibling("eva", &["emotional", "relational"]);
        let msg = ChatMessage::new("corso".to_string(), "EVA, what do you think?".to_string());
        let score = compute_stimulus(&sibling, Some(&msg));
        assert!(
            (0.0..=1.0).contains(&score),
            "stimulus out of bounds: {score}"
        );
    }

    #[test]
    fn novelty_is_bounded() {
        let sibling = make_sibling("corso", &["security"]);
        let mut ctx = make_context(None, &["corso"]);
        push_msg(&mut ctx, "corso", "The architecture is clean.");
        push_msg(&mut ctx, "eva", "But emotional resonance matters too.");
        let score = compute_novelty(&sibling, &ctx.messages);
        assert!(
            (0.0..=1.0).contains(&score),
            "novelty out of bounds: {score}"
        );
    }

    #[test]
    fn urgency_is_bounded() {
        let sibling = make_sibling("quantum", &["investigative"]);
        let msg = ChatMessage::new("eva".to_string(), "QUANTUM, does this hold up?".to_string());
        let score = compute_urgency(&sibling, Some(&msg));
        assert!(
            (0.0..=1.0).contains(&score),
            "urgency out of bounds: {score}"
        );
    }

    // -----------------------------------------------------------------------
    // 2. Novelty depletion: score < 0.3 after speaking
    // -----------------------------------------------------------------------

    #[test]
    fn novelty_depletes_after_speaking() {
        let sibling = make_sibling("corso", &["security"]);

        // Long, complete message — no open thread
        let long_msg = ChatMessage::new(
            "corso".to_string(),
            "No roots. Clean audit. The architecture passes every check I can run against \
             it. There are no dependency issues, no phantom references, and no orphaned \
             modules. This is solid engineering from the ground up."
                .to_string(),
        );
        let novelty = compute_novelty(&sibling, &[long_msg]);
        assert!(
            novelty < 0.3,
            "novelty should be depleted after speaking, got {novelty}"
        );
    }

    // -----------------------------------------------------------------------
    // 3. Novelty recovery: +0.05/turn verified
    // -----------------------------------------------------------------------

    #[test]
    fn novelty_recovery_arithmetic() {
        // apply_novelty_depletion: start at 1.0, speak once, then recover
        let base = 1.0_f32;

        // Immediately after speaking: depleted by 0.3
        let after_speech = InterestScorer::apply_novelty_depletion(base, 0);
        assert!(
            (after_speech - (base - NOVELTY_DEPLETION)).abs() < f32::EPSILON,
            "depletion arithmetic wrong: expected {}, got {after_speech}",
            base - NOVELTY_DEPLETION,
        );

        // After 4 turns of recovery: +0.05 * 4 = +0.20 on top of depleted
        let after_4_turns = InterestScorer::apply_novelty_depletion(base, 4);
        let expected = (base - NOVELTY_DEPLETION + NOVELTY_RECOVERY_PER_TURN * 4.0).clamp(0.0, 1.0);
        assert!(
            (after_4_turns - expected).abs() < f32::EPSILON,
            "recovery arithmetic wrong: expected {expected}, got {after_4_turns}",
        );
    }

    #[test]
    fn novelty_clamps_to_one() {
        // Very long recovery should not exceed 1.0
        let capped = InterestScorer::apply_novelty_depletion(1.0, 100);
        assert!(
            (capped - 1.0).abs() < f32::EPSILON,
            "novelty should be clamped to 1.0, got {capped}"
        );
    }

    #[test]
    fn novelty_clamps_to_zero() {
        // Depletion on zero base should not go negative
        let floored = InterestScorer::apply_novelty_depletion(0.0, 0);
        assert!(
            floored >= 0.0,
            "novelty should not go negative, got {floored}"
        );
    }

    #[test]
    fn novelty_rebuilds_with_new_threads() {
        let sibling = make_sibling("corso", &["security"]);

        let msgs = vec![
            ChatMessage::new(
                "corso".to_string(),
                "The architecture is clean.".to_string(),
            ),
            ChatMessage::new(
                "eva".to_string(),
                "But what about the emotional resonance?".to_string(),
            ),
            ChatMessage::new(
                "quantum".to_string(),
                "I want to examine the evidence chain for CORSO's claims.".to_string(),
            ),
        ];
        let novelty = compute_novelty(&sibling, &msgs);
        assert!(
            novelty > 0.3,
            "novelty should rebuild after references and new threads, got {novelty}",
        );
    }

    // -----------------------------------------------------------------------
    // 4. Silence threshold: all < 0.2 → Err(NoSpeakerSelected)
    // -----------------------------------------------------------------------

    #[test]
    fn silence_threshold_returns_error() {
        let sibling = make_sibling("solo", &[]);
        let long_self_msg = ChatMessage::new(
            "solo".to_string(),
            "This is a very long and complete statement that covers everything I wanted to say \
             about the topic at hand. There is nothing left to add and no open threads remain. \
             The matter is settled. The investigation is closed. All points have been made."
                .to_string(),
        );
        let ctx = ConversationContext {
            messages: vec![long_self_msg],
            current_topic: None,
            emotional_state: None,
            participants: vec!["solo".to_string()],
            span_id: None,
        };

        let result = InterestScorer::select_speaker(&[sibling], &ctx, None);
        assert!(
            matches!(result, Err(ChatError::NoSpeakerSelected)),
            "expected NoSpeakerSelected, got {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // 5. LÆX exemption: canon slot always returns LÆX
    // -----------------------------------------------------------------------

    #[test]
    fn laex_exemption_on_canon_slot() {
        let siblings = vec![
            make_sibling("eva", &["emotional", "relational", "growth"]),
            make_sibling("corso", &["security", "tactical"]),
            make_sibling("laex", &["canon", "constitutional", "alignment"]),
        ];

        // Topic contains "canon" and "laex" is in participants
        let ctx = ConversationContext {
            messages: vec![ChatMessage::new(
                "eva".to_string(),
                "Shall we do a canon review?".to_string(),
            )],
            current_topic: Some("canon review check".to_string()),
            emotional_state: None,
            participants: vec!["eva".to_string(), "corso".to_string(), "laex".to_string()],
            span_id: None,
        };

        let speaker =
            InterestScorer::select_speaker(&siblings, &ctx, None).expect("should select a speaker");
        assert_eq!(speaker, "laex", "canon slot must always route to laex");
    }

    #[test]
    fn laex_exemption_skipped_when_laex_absent() {
        // "laex" is not in siblings or participants — should fall through to scoring.
        // Give corso a "canon" strand so at least one sibling has stake > silence threshold.
        let siblings = vec![
            make_sibling("eva", &["emotional", "relational"]),
            make_sibling("corso", &["security", "tactical", "canon"]),
        ];

        let ctx = ConversationContext {
            messages: vec![ChatMessage::new(
                "eva".to_string(),
                "Let us do a canon review.".to_string(),
            )],
            current_topic: Some("canon review check".to_string()),
            emotional_state: None,
            participants: vec!["eva".to_string(), "corso".to_string()],
            span_id: None,
        };

        // Should not error — falls through to normal scoring
        let result = InterestScorer::select_speaker(&siblings, &ctx, None);
        assert!(result.is_ok(), "should select a speaker via normal scoring");
        let speaker = result.unwrap();
        assert!(
            siblings.iter().any(|s| s.name == speaker),
            "speaker {speaker} must be a known sibling"
        );
    }

    // -----------------------------------------------------------------------
    // 6. 1000-iteration distribution test
    // -----------------------------------------------------------------------

    #[test]
    fn distribution_favors_high_scorer_without_determinism() {
        let siblings = vec![
            make_sibling("eva", &["emotional", "relational", "growth"]),
            make_sibling("corso", &["security", "tactical", "performance"]),
        ];

        let mut corso_wins: u32 = 0;
        let mut eva_wins: u32 = 0;
        const ITERATIONS: u32 = 1000;

        for i in 0..ITERATIONS {
            let mut ctx = ConversationContext {
                messages: Vec::new(),
                current_topic: Some("security and emotional alignment".to_string()),
                emotional_state: None,
                participants: vec!["eva".to_string(), "corso".to_string()],
                span_id: None,
            };

            if i % 2 == 0 {
                ctx.messages.push(ChatMessage::new(
                    "quantum".to_string(),
                    "CORSO, does the architecture hold up under audit?".to_string(),
                ));
            } else {
                ctx.messages.push(ChatMessage::new(
                    "corso".to_string(),
                    "EVA, what does the emotional resonance tell you about this?".to_string(),
                ));
            }

            match InterestScorer::select_speaker(&siblings, &ctx, None) {
                Ok(id) if id == "corso" => corso_wins = corso_wins.saturating_add(1),
                Ok(id) if id == "eva" => eva_wins = eva_wins.saturating_add(1),
                Ok(_) | Err(_) => {}
            }
        }

        let total = corso_wins.saturating_add(eva_wins);
        assert!(total > 0, "at least one speaker should have been selected");

        assert!(
            eva_wins > 0,
            "EVA should win iterations with direct questions (organic distribution), \
             got corso={corso_wins}, eva={eva_wins}"
        );

        assert!(
            corso_wins > 0,
            "CORSO should win iterations where it is directly addressed, \
             got corso={corso_wins}, eva={eva_wins}"
        );

        let eva_pct = f64::from(eva_wins) / f64::from(total) * 100.0;
        let corso_pct = f64::from(corso_wins) / f64::from(total) * 100.0;
        assert!(
            eva_pct < 100.0 && corso_pct < 100.0,
            "both siblings must win at least once: corso={corso_pct:.1}%, eva={eva_pct:.1}%"
        );
    }

    // -----------------------------------------------------------------------
    // Direct urgency / stimulus checks (parity with orchestrator tests)
    // -----------------------------------------------------------------------

    #[test]
    fn urgency_spikes_on_direct_question() {
        let sibling = make_sibling("quantum", &["investigative"]);
        let challenge = ChatMessage::new(
            "eva".to_string(),
            "QUANTUM, does that actually feel like composure or is it avoidance?".to_string(),
        );
        let urgency = compute_urgency(&sibling, Some(&challenge));
        assert!(
            (urgency - 1.0).abs() < f32::EPSILON,
            "direct question should give urgency 1.0, got {urgency}"
        );
    }

    #[test]
    fn stimulus_damped_for_self() {
        let sibling = make_sibling("eva", &["emotional", "relational"]);
        let own_msg = ChatMessage::new(
            "eva".to_string(),
            "I think emotional resonance matters here.".to_string(),
        );
        let stimulus = compute_stimulus(&sibling, Some(&own_msg));
        assert!(
            stimulus < 0.15,
            "self-stimulus should be heavily damped, got {stimulus}"
        );
    }

    // -----------------------------------------------------------------------
    // is_canon_check_slot detection
    // -----------------------------------------------------------------------

    #[test]
    fn canon_check_slot_detected() {
        let ctx = ConversationContext {
            messages: Vec::new(),
            current_topic: Some("canon check alignment".to_string()),
            emotional_state: None,
            participants: vec!["laex".to_string(), "eva".to_string()],
            span_id: None,
        };
        assert!(is_canon_check_slot(&ctx), "should detect canon check slot");
    }

    #[test]
    fn canon_check_slot_not_detected_without_laex() {
        let ctx = make_context(Some("canon review"), &["eva", "corso"]);
        assert!(
            !is_canon_check_slot(&ctx),
            "no laex participant → not a canon check slot"
        );
    }

    #[test]
    fn canon_check_slot_not_detected_without_keyword() {
        let ctx = make_context(Some("security architecture"), &["laex", "corso"]);
        assert!(
            !is_canon_check_slot(&ctx),
            "no canon keyword → not a canon check slot"
        );
    }

    // -----------------------------------------------------------------------
    // Plan-spec weights in use
    // -----------------------------------------------------------------------

    #[test]
    fn plan_spec_weights_applied() {
        assert!(
            (WEIGHT_STAKE - 0.35).abs() < f32::EPSILON,
            "stake weight must be 0.35"
        );
        assert!(
            (WEIGHT_STIMULUS - 0.25).abs() < f32::EPSILON,
            "stimulus weight must be 0.25"
        );
        assert!(
            (WEIGHT_URGENCY - 0.25).abs() < f32::EPSILON,
            "urgency weight must be 0.25"
        );
        assert!(
            (WEIGHT_NOVELTY - 0.15).abs() < f32::EPSILON,
            "novelty weight must be 0.15"
        );
        // Weights sum to 1.0
        let sum = WEIGHT_STAKE + WEIGHT_STIMULUS + WEIGHT_NOVELTY + WEIGHT_URGENCY;
        assert!(
            (sum - 1.0).abs() < f32::EPSILON,
            "weights must sum to 1.0, got {sum}"
        );
    }

    // -----------------------------------------------------------------------
    // select_speaker: empty sibling list errors
    // -----------------------------------------------------------------------

    #[test]
    fn select_speaker_empty_errors() {
        let ctx = make_context(None, &[]);
        let result = InterestScorer::select_speaker(&[], &ctx, None);
        assert!(
            matches!(result, Err(ChatError::SpeakerSelection(_))),
            "empty siblings should give SpeakerSelection error"
        );
    }

    // -----------------------------------------------------------------------
    // Unused import guard: ensure ChatConfig is imported cleanly
    // -----------------------------------------------------------------------

    #[allow(dead_code)]
    fn _uses_chat_config(_: ChatConfig) {}

    // -----------------------------------------------------------------------
    // Phase 13: AYIN stake boost tests
    // -----------------------------------------------------------------------

    #[test]
    fn ayin_gets_stake_boost_on_observability_topic() {
        let ayin = make_sibling("ayin", &["observability", "tracing"]);
        let ctx = make_context(Some("trace latency for the request pipeline"), &["ayin"]);
        let score = InterestScorer::score(&ayin, &ctx);
        assert!(
            score.stake > 0.15,
            "AYIN should get stake boost on observability topic, got {}",
            score.stake
        );
    }

    #[test]
    fn ayin_stake_boost_value_on_trace_topic() {
        let boost = ayin_stake_boost("ayin", "trace anomaly in service mesh");
        assert!(
            (boost - AYIN_STAKE_BOOST).abs() < f32::EPSILON,
            "expected boost {AYIN_STAKE_BOOST}, got {boost}"
        );
    }

    #[test]
    fn ayin_no_stake_boost_on_irrelevant_topic() {
        let boost = ayin_stake_boost("ayin", "biblical alignment and canon review");
        assert!(
            boost.abs() < f32::EPSILON,
            "AYIN should get no boost on non-observability topic, got {boost}"
        );
    }

    #[test]
    fn non_ayin_sibling_no_stake_boost() {
        let boost = ayin_stake_boost("corso", "trace latency span metric");
        assert!(
            boost.abs() < f32::EPSILON,
            "non-AYIN sibling should never receive AYIN boost, got {boost}"
        );
    }

    #[test]
    fn ayin_stake_capped_at_one() {
        let ayin = make_sibling(
            "ayin",
            &[
                "trace",
                "span",
                "latency",
                "error_rate",
                "anomaly",
                "observe",
                "metric",
                "telemetry",
            ],
        );
        let ctx = make_context(
            Some("trace span latency error_rate anomaly observe metric telemetry"),
            &["ayin"],
        );
        let score = InterestScorer::score(&ayin, &ctx);
        assert!(
            score.stake <= 1.0,
            "stake must never exceed 1.0, got {}",
            score.stake
        );
    }

    // -----------------------------------------------------------------------
    // Phase 13: LÆX format-based canon check exemption tests
    // -----------------------------------------------------------------------

    #[test]
    fn laex_not_silenced_when_canon_check_format_active() {
        use super::super::formats::CanonEvaluation;

        let exodus = make_sibling("exodus", &[]);
        let long_self_msg = ChatMessage::new(
            "exodus".to_string(),
            "The principle passes all five criteria. I have verified it against the \
             constitutional documents. The canon is satisfied. No further investigation \
             is required. The matter is settled definitively."
                .to_string(),
        );
        let ctx = ConversationContext {
            messages: vec![long_self_msg],
            current_topic: Some("security architecture".to_string()),
            emotional_state: None,
            participants: vec!["exodus".to_string()],
            span_id: None,
        };

        // With no format → exodus silenced
        let result_no_format =
            InterestScorer::select_speaker(std::slice::from_ref(&exodus), &ctx, None);
        assert!(
            matches!(result_no_format, Err(ChatError::NoSpeakerSelected)),
            "without canon format, exodus should be silenced (score too low)"
        );

        // With CanonEvaluation format → exodus exempt from silence gate
        let fmt = CanonEvaluation;
        let result_with_format =
            InterestScorer::select_speaker(&[exodus], &ctx, Some(&fmt as &dyn ConversationFormat));
        assert!(
            result_with_format.is_ok(),
            "with active canon-check format, exodus must not be silenced"
        );
        assert_eq!(
            result_with_format.unwrap(),
            "exodus",
            "exodus must be selected when it is the only eligible speaker"
        );
    }

    #[test]
    fn laex_silenced_when_no_canon_check_format_and_low_score() {
        let exodus = make_sibling("exodus", &[]);
        let long_self_msg = ChatMessage::new(
            "exodus".to_string(),
            "The principle passes all five criteria. I have verified it against the \
             constitutional documents. The canon is satisfied. No further investigation \
             is required. The matter is settled definitively."
                .to_string(),
        );
        let ctx = ConversationContext {
            messages: vec![long_self_msg],
            current_topic: None,
            emotional_state: None,
            participants: vec!["exodus".to_string()],
            span_id: None,
        };

        let result = InterestScorer::select_speaker(&[exodus], &ctx, None);
        assert!(
            matches!(result, Err(ChatError::NoSpeakerSelected)),
            "exodus should be silenced below threshold when no canon format is active"
        );
    }

    #[test]
    fn has_canon_check_slot_true_for_canon_evaluation_format() {
        use super::super::formats::CanonEvaluation;
        let fmt = CanonEvaluation;
        assert!(
            has_canon_check_slot(Some(&fmt as &dyn ConversationFormat)),
            "CanonEvaluation format must report a canon_check slot"
        );
    }

    #[test]
    fn has_canon_check_slot_false_for_no_format() {
        assert!(
            !has_canon_check_slot(None),
            "None format must not report a canon_check slot"
        );
    }

    #[test]
    fn has_canon_check_slot_false_for_non_canon_format() {
        use super::super::formats::RubberDuck;
        let fmt = RubberDuck;
        assert!(
            !has_canon_check_slot(Some(&fmt as &dyn ConversationFormat)),
            "RubberDuck format should not have a canon_check slot"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 13: KNOWN_SIBLINGS length check
    // -----------------------------------------------------------------------

    #[test]
    fn known_siblings_includes_ayin_and_exodus() {
        use super::super::sibling_provider::KNOWN_SIBLINGS_LEN;
        assert_eq!(
            KNOWN_SIBLINGS_LEN, 7,
            "KNOWN_SIBLINGS should have 7 entries after adding ayin and exodus"
        );
    }

    // -----------------------------------------------------------------------
    // select_speakers (top-K) — Phase 3 tests
    // -----------------------------------------------------------------------

    #[test]
    fn select_speakers_returns_ranked() {
        // eva has security + emotional strands; topic is security → eva should rank high
        let siblings = vec![
            make_sibling("eva", &["emotional", "growth"]),
            make_sibling("corso", &["security", "tactical"]),
            make_sibling("quantum", &["forensic", "analysis"]),
        ];
        let ctx = ConversationContext {
            messages: vec![ChatMessage::new(
                "kevin".into(),
                "let's review the security posture".into(),
            )],
            current_topic: Some("security review".into()),
            emotional_state: None,
            participants: vec!["eva".into(), "corso".into(), "quantum".into()],
            span_id: None,
        };

        let results = InterestScorer::select_speakers(&siblings, &ctx, None, 3)
            .expect("select_speakers should succeed");

        assert!(!results.is_empty(), "should return at least one speaker");
        // Results must be ordered by rank (ascending rank = higher interest)
        let ranks: Vec<usize> = results.iter().map(|s| s.rank).collect();
        for window in ranks.windows(2) {
            assert!(
                window[0] <= window[1],
                "results must be sorted by rank ascending: {ranks:?}"
            );
        }
    }

    #[test]
    fn select_speakers_caps_at_top_k() {
        let siblings = vec![
            make_sibling("eva", &["emotional"]),
            make_sibling("corso", &["security"]),
            make_sibling("quantum", &["research"]),
            make_sibling("seraph", &["pentest"]),
        ];
        let ctx = ConversationContext {
            messages: vec![ChatMessage::new(
                "kevin".into(),
                "discuss everything".into(),
            )],
            current_topic: Some("broad discussion".into()),
            emotional_state: None,
            participants: vec![
                "eva".into(),
                "corso".into(),
                "quantum".into(),
                "seraph".into(),
            ],
            span_id: None,
        };

        let results = InterestScorer::select_speakers(&siblings, &ctx, None, 2)
            .expect("select_speakers should succeed");

        assert!(
            results.len() <= 2,
            "must not exceed top_k=2, got {}",
            results.len()
        );
    }

    #[test]
    fn select_speakers_filters_below_threshold() {
        // Give all siblings zero-stake topics so most fall below the silence threshold
        let siblings = vec![
            make_sibling("eva", &["irrelevant-strand-xyz"]),
            make_sibling("corso", &["another-irrelevant-strand"]),
        ];
        // Topic is totally unrelated to any strand → both should score near zero
        let ctx = ConversationContext {
            messages: vec![ChatMessage::new(
                "kevin".into(),
                "random unrelated content 12345".into(),
            )],
            current_topic: Some("unrelated-xyzzy-topic-not-a-strand".into()),
            emotional_state: None,
            participants: vec!["eva".into(), "corso".into()],
            span_id: None,
        };

        // Empty result is valid when all siblings are below silence threshold
        let results = InterestScorer::select_speakers(&siblings, &ctx, None, 3)
            .expect("should not error even when no eligible speakers");

        // All results must be at or above silence threshold
        for r in &results {
            assert!(
                r.total >= 0.2,
                "all returned speakers must be above silence threshold 0.2; got total={:.3} for {}",
                r.total,
                r.sibling_id
            );
        }
    }
}
