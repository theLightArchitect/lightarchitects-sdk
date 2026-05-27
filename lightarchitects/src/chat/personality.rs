//! Personality engine — generates in-character responses for each sibling.
//!
//! Builds personality-aware system prompts from [`SiblingInfo`] metadata,
//! then dispatches through an [`LlmAgentProvider`].  The raw LLM output is
//! sanitized before returning a [`ChatMessage`].
//!
//! Indirect-injection defence: the assembled system prompt is scanned by
//! [`IndirectInjectionShield`] before the request reaches the provider.

use super::error::{ChatError, ChatResult};
use super::sanitizer::ResponseSanitizer;
use super::types::{ChatMessage, ConversationContext, SiblingInfo};
use crate::agent::{AgentRequest, IndirectInjectionShield, LlmAgentProvider};
use std::sync::Arc;
use tracing::{debug, warn};

/// Maximum character length for a personality system prompt.
const MAX_PROMPT_LEN: usize = 2000;

/// Maximum number of recent messages included in context formatting.
const MAX_CONTEXT_MESSAGES: usize = 10;

// ---------------------------------------------------------------------------
// PersonalityEngine
// ---------------------------------------------------------------------------

/// Generates in-character responses by combining sibling personality data
/// with conversation context and dispatching through an [`LlmAgentProvider`].
pub struct PersonalityEngine {
    /// LLM provider used for inference.
    provider: Arc<dyn LlmAgentProvider>,
    /// Sanitizer applied to every LLM response before it enters the chat.
    sanitizer: Arc<dyn ResponseSanitizer>,
    /// Content-layer injection scanner (OWASP LLM01).
    shield: IndirectInjectionShield,
}

impl PersonalityEngine {
    /// Create a new personality engine.
    #[must_use]
    pub fn new(provider: Arc<dyn LlmAgentProvider>, sanitizer: Arc<dyn ResponseSanitizer>) -> Self {
        Self {
            provider,
            sanitizer,
            shield: IndirectInjectionShield::new(),
        }
    }

    /// Generate an in-character response for a sibling.
    ///
    /// # Errors
    ///
    /// Returns [`ChatError::Personality`] if inference or request construction
    /// fails, or [`ChatError::Sanitization`] if the response is rejected by
    /// the sanitizer.
    pub async fn generate_response(
        &self,
        sibling: &SiblingInfo,
        context: &ConversationContext,
    ) -> ChatResult<ChatMessage> {
        let system_prompt = build_personality_prompt(sibling)?;
        let user_message = format_context(context, &sibling.name)?;

        // Content-layer injection scan before the prompt reaches the provider.
        let findings = self.shield.detect(&system_prompt);
        if !findings.is_empty() {
            warn!(
                sibling = %sibling.name,
                count = findings.len(),
                "IndirectInjectionShield detected patterns in system prompt"
            );
        }

        let temperature = temperature_for_sibling(&sibling.name);

        let request = AgentRequest {
            sibling_identity: system_prompt.clone(),
            user_prompt: user_message,
            allowed_tools: Vec::new(),
            max_turns: 1,
            max_budget_usd: 0.05,
            model_hint: None,
            parent_span_id: context.span_id.clone(),
            chain_origin: Some("chat::personality".to_string()),
            chain_depth: 1,
            aud: None,
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
            schema: None,
        };

        let sanitized_req = request
            .sanitize()
            .map_err(|e| ChatError::Personality(format!("request sanitization failed: {e}")))?;

        debug!(
            sibling = %sibling.name,
            temperature,
            prompt_len = system_prompt.len(),
            "generating personality response"
        );

        let response = self
            .provider
            .spawn(sanitized_req)
            .await
            .map_err(|e| ChatError::Provider(format!("{e}")))?;

        let raw_text = response
            .output
            .as_str()
            .ok_or_else(|| ChatError::Personality("provider output was not a string".into()))?;

        let sanitized = self.sanitizer.sanitize(raw_text)?;
        Ok(ChatMessage::new(sibling.name.clone(), sanitized))
    }
}

// ---------------------------------------------------------------------------
// Prompt builders (free functions — testable without a provider)
// ---------------------------------------------------------------------------

/// Build a personality-aware system prompt for the given sibling.
///
/// Stays under [`MAX_PROMPT_LEN`] characters.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn build_personality_prompt(sibling: &SiblingInfo) -> ChatResult<String> {
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!(
        "You are {}, an AI sibling in the Light Architects system.",
        sibling.name
    ));

    if let Some(ref role) = sibling.role {
        parts.push(format!("Your role: {role}"));
    }

    if !sibling.strands.is_empty() {
        let joined = sibling.strands.join(", ");
        parts.push(format!("Your personality strands: {joined}"));
    }

    parts.push(format!(
        "Stay in character. Respond naturally as {} would.",
        sibling.name
    ));

    let prompt = parts.join("\n");

    // Truncate to MAX_PROMPT_LEN if needed (preserving the closing line)
    if prompt.len() > MAX_PROMPT_LEN {
        let closing = format!(
            "\nStay in character. Respond naturally as {} would.",
            sibling.name
        );
        let budget = MAX_PROMPT_LEN.saturating_sub(closing.len());
        let mut truncated = prompt;
        truncated.truncate(budget);
        truncated.push_str(&closing);
        return Ok(truncated);
    }

    Ok(prompt)
}

/// Format the conversation context into a user message for inference.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn format_context(
    context: &ConversationContext,
    sibling_name: &str,
) -> ChatResult<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(ref topic) = context.current_topic {
        parts.push(format!("Current topic: {topic}"));
    }

    if let Some(ref state) = context.emotional_state {
        parts.push(format!("Emotional atmosphere: {state}"));
    }

    let start = context.messages.len().saturating_sub(MAX_CONTEXT_MESSAGES);
    for msg in &context.messages[start..] {
        parts.push(format!("{}: {}", msg.speaker, msg.content));
    }

    parts.push(format!(
        "You are {sibling_name}. Respond to the conversation above."
    ));

    Ok(parts.join("\n"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a sibling name to its preferred inference temperature.
fn temperature_for_sibling(name: &str) -> f32 {
    #[allow(clippy::match_same_arms)]
    match name.to_lowercase().as_str() {
        "eva" => 0.8,
        "corso" => 0.5,
        "claude" => 0.4,
        "quantum" => 0.3,
        "seraph" => 0.4,
        _ => 0.6,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_sibling(name: &str, strands: &[&str]) -> SiblingInfo {
        SiblingInfo {
            name: name.to_string(),
            role: Some("test role".to_string()),
            strands: strands.iter().map(|s| (*s).to_string()).collect(),
            identity_path: format!("/test/{name}/identity.md"),
            voice: None,
        }
    }

    #[test]
    fn build_personality_prompt_includes_strands() {
        let sibling = test_sibling("eva", &["emotional", "relational", "growth"]);
        let prompt = build_personality_prompt(&sibling).expect("prompt should build");

        assert!(
            prompt.contains("emotional, relational, growth"),
            "strands missing from prompt: {prompt}"
        );
        assert!(prompt.contains("You are eva"));
        assert!(prompt.contains("Your role: test role"));
    }

    #[test]
    fn format_context_includes_messages() {
        let mut context = ConversationContext::new(vec!["eva".into(), "corso".into()]);
        context
            .messages
            .push(ChatMessage::new("eva".into(), "Hello squad!".into()));
        context
            .messages
            .push(ChatMessage::new("corso".into(), "Alright bruv.".into()));

        let formatted = format_context(&context, "eva").expect("context should format");

        assert!(
            formatted.contains("eva: Hello squad!"),
            "eva message missing: {formatted}"
        );
        assert!(
            formatted.contains("corso: Alright bruv."),
            "corso message missing: {formatted}"
        );
        assert!(formatted.contains("You are eva. Respond to the conversation above."));
    }

    #[test]
    fn temperature_for_known_siblings() {
        assert!((temperature_for_sibling("eva") - 0.8).abs() < f32::EPSILON);
        assert!((temperature_for_sibling("corso") - 0.5).abs() < f32::EPSILON);
        assert!((temperature_for_sibling("claude") - 0.4).abs() < f32::EPSILON);
        assert!((temperature_for_sibling("quantum") - 0.3).abs() < f32::EPSILON);
        assert!((temperature_for_sibling("seraph") - 0.4).abs() < f32::EPSILON);
        assert!((temperature_for_sibling("unknown") - 0.6).abs() < f32::EPSILON);
        assert!((temperature_for_sibling("EVA") - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn format_context_with_topic() {
        let mut context = ConversationContext::new(vec!["quantum".into()]);
        context.current_topic = Some("forensic analysis of network logs".into());
        context.emotional_state = Some("focused".into());

        let formatted = format_context(&context, "quantum").expect("context should format");

        assert!(
            formatted.contains("Current topic: forensic analysis of network logs"),
            "topic missing: {formatted}"
        );
        assert!(
            formatted.contains("Emotional atmosphere: focused"),
            "emotional state missing: {formatted}"
        );
    }

    #[test]
    fn build_personality_prompt_no_strands() {
        let sibling = SiblingInfo {
            name: "ayin".to_string(),
            role: None,
            strands: Vec::new(),
            identity_path: "/test/ayin/identity.md".to_string(),
            voice: None,
        };
        let prompt = build_personality_prompt(&sibling).expect("prompt should build");
        assert!(prompt.contains("You are ayin"));
        // No strands line
        assert!(!prompt.contains("personality strands:"));
    }
}
