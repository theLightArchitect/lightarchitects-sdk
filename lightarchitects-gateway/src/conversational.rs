//! Conversational build mode — pair programmer in a box.
//!
//! Accumulates messages, streams LLM responses, and can promote context
//! into a formal LASDLC build plan when the user signals "let's build this".

use std::collections::VecDeque;

/// Accumulated message in a conversational session.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// "user" or "assistant"
    pub role: String,
    /// Message body.
    pub content: String,
}

/// Conversational session — no formal build plan until promoted.
///
/// Designed to be lightweight and synchronous at this layer; LLM streaming
/// is injected by the caller via [`ConversationalSession::push_assistant`].
#[derive(Debug, Clone, PartialEq)]
pub struct ConversationalSession {
    history: VecDeque<Message>,
    max_tokens: usize,
}

impl Default for ConversationalSession {
    fn default() -> Self {
        Self {
            history: VecDeque::new(),
            max_tokens: 32_000,
        }
    }
}

impl ConversationalSession {
    /// Create a new session with a token ceiling.
    #[must_use]
    pub fn new(max_tokens: usize) -> Self {
        Self {
            history: VecDeque::new(),
            max_tokens,
        }
    }

    /// Push a user message into the session.
    pub fn push_user(&mut self, prompt: &str) {
        self.history.push_back(Message {
            role: "user".into(),
            content: prompt.into(),
        });
    }

    /// Push an assistant response into the session.
    pub fn push_assistant(&mut self, response: &str) {
        self.history.push_back(Message {
            role: "assistant".into(),
            content: response.into(),
        });
    }

    /// Return the full conversation history (oldest first).
    #[must_use]
    pub fn history(&self) -> &VecDeque<Message> {
        &self.history
    }

    /// Produce a LASDLC-style plan from accumulated user prompts.
    ///
    /// Returns `None` when the session is empty or contains no user messages.
    #[must_use]
    pub fn suggest_plan(&self) -> Option<String> {
        let goals: Vec<&str> = self
            .history
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .collect();

        if goals.is_empty() {
            return None;
        }

        let plan = format!(
            "LASDLC Plan\n===========\nGoals:\n- {}\n\nPhase 1 — Discover\nPhase 2 — Build\nPhase 3 — Verify\nPhase 4 — Deploy\n",
            goals.join("\n- ")
        );
        Some(plan)
    }

    /// Rough token estimate (1 token ≈ 4 bytes for ASCII/UTF-8 text).
    #[must_use]
    pub fn token_estimate(&self) -> usize {
        self.history.iter().map(|m| m.content.len() / 4).sum()
    }

    /// True if the accumulated context is under the token ceiling.
    #[must_use]
    pub fn within_budget(&self) -> bool {
        self.token_estimate() <= self.max_tokens
    }

    /// True if there are no messages yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]
mod tests {
    use super::*;

    #[test]
    fn brainstorm_accepts_open_ended_prompt() {
        let mut session = ConversationalSession::default();
        session.push_user("how do I refactor auth middleware?");
        session.push_assistant("start by extracting the token validation logic into its own module.");
        assert_eq!(session.history().len(), 2);
    }

    #[test]
    fn suggest_plan_extracts_goals_from_context() {
        let mut session = ConversationalSession::default();
        session.push_user("I need a login page");
        session.push_assistant("What stack?");
        session.push_user("React + Tailwind");

        let plan = session.suggest_plan();
        let Some(plan) = plan else {
            panic!("plan should be generated after 3 messages (suggest_plan returned None)");
        };
        assert!(plan.contains("login page"), "plan should include first user goal");
        assert!(plan.contains("React + Tailwind"), "plan should include second user goal");
        assert!(plan.contains("LASDLC Plan"), "plan should have LASDLC header");
    }

    #[test]
    fn suggest_plan_returns_none_when_empty() {
        let session = ConversationalSession::default();
        assert_eq!(session.suggest_plan(), None);
    }

    #[test]
    fn context_accumulation_no_truncation_under_32k() {
        let mut session = ConversationalSession::default();
        for i in 0..10 {
            session.push_user(&format!("message {i} with some padding to simulate real content"));
            session.push_assistant(&format!("response {i} with matching padding so tokens add up"));
        }
        assert!(session.within_budget(), "10 messages should fit under 32k ceiling");
    }

    #[test]
    fn token_estimate_is_monotonic() {
        let mut session = ConversationalSession::default();
        let t0 = session.token_estimate();
        session.push_user("hello");
        let t1 = session.token_estimate();
        assert!(t1 > t0, "token estimate should grow after user message");
        session.push_assistant("hi there");
        let t2 = session.token_estimate();
        assert!(t2 > t1, "token estimate should grow after assistant message");
    }

    #[test]
    fn conversational_does_not_spawn_agents_until_plan_promoted() {
        let mut session = ConversationalSession::default();
        session.push_user("let's build a cache layer");
        session.push_assistant("Redis or in-memory?");
        // No subprocesses, no agents — just messages.
        assert!(session.suggest_plan().is_some());
        assert_eq!(session.history().len(), 2);
    }
}
