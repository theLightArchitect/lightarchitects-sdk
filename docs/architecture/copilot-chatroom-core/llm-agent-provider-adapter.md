# LlmAgentProvider Adapter — Locked Signature

> Phase 1 architecture artifact. Adapter design for PersonalityEngine
> adapting from neural-engine::TierRouter to LlmAgentProvider.

## The binding constraint

`PersonalityEngine` currently uses `neural_engine::router::TierRouter`:
```rust
// soul-chat/src/personality.rs:30
pub struct PersonalityEngine {
    router: Arc<TierRouter>,
    sanitizer: ResponseSanitizer,
}
```

After Phase 2 extraction, it uses `LlmAgentProvider` directly:
```rust
// lightarchitects/src/chat/personality.rs (Phase 2)
pub struct PersonalityEngine {
    provider: Arc<dyn LlmAgentProvider>,
    sanitizer: ResponseSanitizer,
}
```

## Locked adapter signature

The translation from personality request to `SanitizedAgentRequest`:

```rust
/// Build a sanitized agent request from sibling personality data.
///
/// # Safety invariant
/// identity.md content is untrusted (user-controlled via vault).
/// IndirectInjectionShield must screen it before construction.
fn build_sanitized_request(
    sibling: &SiblingInfo,
    ctx: &ConversationContext,
    history: &[ChatMessage],
    shield: &IndirectInjectionShield,
) -> Result<SanitizedAgentRequest, ChatError> {
    // 1. Build system prompt from sibling personality metadata
    let system = build_personality_prompt(sibling, ctx)?;

    // 2. Screen identity content through injection shield (OWASP LLM01)
    let screened_system = shield
        .screen_content(&system)
        .map_err(|_| ChatError::InjectionDetected)?;

    // 3. Build AgentRequest — provider validates budget/turns
    let req = AgentRequest {
        identity: screened_system,       // sibling persona system prompt
        prompt: format_history(history), // conversation history
        max_budget_usd: None,            // personality calls are short
        max_turns: Some(1),              // single-turn per sibling voice
        parent_span_id: ctx.span_id.clone(),
    };

    // 4. Sanitize to prove G1 already applied
    SanitizedAgentRequest::from_raw(req)
        .map_err(|e| ChatError::Sanitization(e.to_string()))
}
```

## Stream mapping

`spawn_streaming()` → `BoxStream<'static, ProviderEvent>` maps to `ChatMessage`:

```rust
async fn generate(&self, sibling: &SiblingInfo, ctx: &ConversationContext,
    history: &[ChatMessage]) -> Result<ChatMessage, ChatError>
{
    let req = build_sanitized_request(sibling, ctx, history, &self.shield)?;
    let mut stream = self.provider.spawn_streaming(req).await
        .map_err(|e| ChatError::Provider(e.to_string()))?;

    let mut content = String::new();
    while let Some(event) = stream.next().await {
        if let ProviderEvent::TextDelta { text, .. } = event {
            content.push_str(&text);
        }
    }

    let content = self.sanitizer.sanitize(&content);
    Ok(ChatMessage {
        role: ChatRole::Assistant,
        content,
        sibling_id: Some(sibling.id.clone()),
        actor: Some(sibling.display_name.clone()),
    })
}
```

## Decision: IndirectInjectionShield placement

Shield is created once per `PersonalityEngine` and shared across all
per-sibling calls. Identity content screened at request-build time (before
the SanitizedAgentRequest newtype wraps it), not at response-parse time.
This is the correct placement per CLAUDE.md feedback_indirect_injection_shield_pattern.md.
