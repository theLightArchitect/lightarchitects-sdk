//! L2 conversation session — SDK promotion of the gateway `AgentRunner`.
//!
//! [`ConversationSession`] wraps an [`LlmAgentProvider`] with structured
//! turn history, lifecycle hooks, and a pluggable [`Transport`] for output.
//! It is the canonical L2 interface for all agent session management in the
//! Light Architects SDK.
//!
//! ## Relationship to `AgentRunner`
//!
//! The gateway `AgentRunner` (`lightarchitects-gateway/src/agent_stream/runner.rs`)
//! is retained as a working implementation; this type is its SDK-promoted
//! counterpart. New code should use [`ConversationSession`]; the gateway shim
//! re-exports this type for gradual migration.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::agent::{AgentRequest, AgentResponse, ChainContext, LlmAgentProvider, ProviderError};

use super::{
    event::{ConversationEvent, TerminationReason},
    memory::{ConversationMemory, InMemoryConversationMemory, MessageRole},
    transport::Transport,
};
use crate::agent::hooks::Hooks;

// ── SessionConfig ─────────────────────────────────────────────────────────────

/// Frozen configuration for a [`ConversationSession`].
///
/// Created once at session construction and not modified during the session
/// lifetime.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Working directory for tool calls.
    pub cwd: PathBuf,
    /// Optional system prompt preamble; overrides provider default when set.
    pub system_prompt: Option<String>,
    /// Maximum provider turns per session turn (0 = provider default).
    pub max_turns: u32,
    /// Hard USD budget cap per session turn.
    pub max_budget_usd: f64,
    /// Optional model hint forwarded to the provider.
    pub model_hint: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        let cwd = std::env::var("HOME").map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from);
        Self {
            cwd,
            system_prompt: None,
            max_turns: 10,
            max_budget_usd: 1.0,
            model_hint: None,
        }
    }
}

// ── SessionState ──────────────────────────────────────────────────────────────

/// Mutable runtime state for a [`ConversationSession`].
#[derive(Debug)]
pub struct SessionState {
    /// Set to `true` by [`ConversationSession::interrupt`] to cancel mid-turn.
    pub interrupt_flag: Arc<AtomicBool>,
    /// Number of turns successfully completed in this session.
    pub turn_count: u32,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            interrupt_flag: Arc::new(AtomicBool::new(false)),
            turn_count: 0,
        }
    }
}

// ── SessionError ──────────────────────────────────────────────────────────────

/// Errors that can occur during a session turn.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Provider returned an error (LLM call failure, budget exceeded, etc.).
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),

    /// Transport write failed.
    #[error("transport I/O error: {0}")]
    Transport(#[from] std::io::Error),

    /// The session was interrupted by an external caller.
    #[error("session interrupted by caller")]
    Interrupted,
}

// ── ConversationSession ───────────────────────────────────────────────────────

/// L2 session manager wrapping an [`LlmAgentProvider`].
///
/// ## Builder pattern
///
/// ```rust,ignore
/// let session = ConversationSession::new(config, provider)
///     .with_memory(Box::new(PersistentMemory::open("session.db")?))
///     .with_hooks(hooks);
/// ```
pub struct ConversationSession<P> {
    /// Frozen configuration (system prompt, cwd, budget).
    pub config: SessionConfig,
    /// Mutable runtime state (interrupt flag, turn counter).
    pub state: SessionState,
    /// Conversation history backend.
    pub memory: Box<dyn ConversationMemory>,
    /// Lifecycle hook registry.
    pub hooks: Hooks,
    provider: Arc<P>,
}

impl<P: LlmAgentProvider> ConversationSession<P> {
    /// Create a session with default in-memory history and no hooks.
    pub fn new(config: SessionConfig, provider: Arc<P>) -> Self {
        Self {
            config,
            state: SessionState::default(),
            memory: Box::new(InMemoryConversationMemory::new()),
            hooks: Hooks::default(),
            provider,
        }
    }

    /// Replace the default in-memory backend with a custom implementation.
    #[must_use]
    pub fn with_memory(mut self, memory: Box<dyn ConversationMemory>) -> Self {
        self.memory = memory;
        self
    }

    /// Attach lifecycle hooks to the session.
    #[must_use]
    pub fn with_hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = hooks;
        self
    }

    /// Signal the in-flight turn to stop at the next iteration boundary.
    pub fn interrupt(&self) {
        self.state.interrupt_flag.store(true, Ordering::SeqCst);
    }

    /// Returns `true` when the interrupt flag is set.
    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        self.state.interrupt_flag.load(Ordering::Relaxed)
    }

    /// Clear a previously set interrupt flag (call before starting a new turn).
    pub fn clear_interrupt(&self) {
        self.state.interrupt_flag.store(false, Ordering::SeqCst);
    }

    // ── Core turn logic ───────────────────────────────────────────────────────

    /// Run one agent turn: user message → provider → events via transport.
    ///
    /// 1. Runs pre-turn hooks.
    /// 2. Appends the user message to memory.
    /// 3. Builds and sanitizes an [`AgentRequest`].
    /// 4. Calls the provider.
    /// 5. Appends the assistant response to memory.
    /// 6. Emits [`ConversationEvent`] values via `transport`.
    /// 7. Runs post-turn hooks.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] on provider failure, transport I/O error, or
    /// if the interrupt flag is set before the turn begins.
    pub async fn run_turn<T: Transport>(
        &mut self,
        user_message: &str,
        transport: &mut T,
        ctx: &ChainContext,
    ) -> Result<AgentResponse, SessionError> {
        if self.is_interrupted() {
            transport
                .emit(&ConversationEvent::Complete {
                    reason: TerminationReason::UserCancelled,
                })
                .await?;
            return Err(SessionError::Interrupted);
        }

        // Pre-turn hooks.
        self.hooks.run_pre_turn(ctx).await;

        // Store user turn.
        self.memory.push(MessageRole::User, user_message.to_owned());

        // Build request from config + session state.
        let system_identity = self
            .config
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful coding assistant.".to_owned());

        let req = AgentRequest {
            sibling_identity: system_identity,
            user_prompt: user_message.to_owned(),
            schema: None,
            allowed_tools: Vec::new(),
            max_turns: self.config.max_turns,
            max_budget_usd: self.config.max_budget_usd,
            model_hint: self.config.model_hint.clone(),
            parent_span_id: None,
            chain_origin: ctx.origin.clone(),
            chain_depth: ctx.depth,
            aud: ctx.aud.clone(),
        };

        let sanitized = req.sanitize()?;

        // Status update while the provider is running.
        transport
            .emit(&ConversationEvent::StatusUpdate {
                text: format!("Calling {} …", self.provider.name()),
            })
            .await?;

        // Call provider.
        let response = self.provider.spawn(sanitized).await?;

        // Extract text output.
        let output_text = match &response.output {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        // Store assistant turn.
        self.memory
            .push(MessageRole::Assistant, output_text.clone());

        // Emit events.
        if !output_text.is_empty() {
            transport
                .emit(&ConversationEvent::Text { chunk: output_text })
                .await?;
        }
        transport
            .emit(&ConversationEvent::TokenUsage {
                input: u64::from(response.tokens.input),
                output: u64::from(response.tokens.output),
            })
            .await?;
        transport
            .emit(&ConversationEvent::Complete {
                reason: TerminationReason::Complete,
            })
            .await?;

        // Post-turn hooks.
        self.hooks.run_post_turn(ctx).await;

        self.state.turn_count += 1;
        Ok(response)
    }

    // ── NDJSON loop (machine-facing) ──────────────────────────────────────────

    /// Read NDJSON [`ControlMessage`]s from stdin and run turns until EOF.
    ///
    /// Mirrors `AgentRunner::run_ndjson_loop`; uses the SDK transport so
    /// downstream consumers see [`ConversationEvent`] values.
    ///
    /// [`ControlMessage`]: crate::agent::conversation::ControlMessage
    pub async fn run_ndjson_loop<T: Transport>(&mut self, transport: &mut T) {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();
        let ctx = ChainContext::default();

        while let Ok(Some(line)) = lines.next_line().await {
            let msg: ControlMessage = match serde_json::from_str(&line) {
                Ok(m) => m,
                Err(e) => {
                    let _ = transport
                        .emit(&ConversationEvent::Error {
                            message: format!("parse error: {e}"),
                            recoverable: Some(true),
                        })
                        .await;
                    continue;
                }
            };

            match msg {
                ControlMessage::SendMessage { text } => {
                    self.clear_interrupt();
                    if let Err(e) = self.run_turn(&text, transport, &ctx).await {
                        let _ = transport
                            .emit(&ConversationEvent::Error {
                                message: e.to_string(),
                                recoverable: Some(false),
                            })
                            .await;
                    }
                }
                ControlMessage::Interrupt => {
                    self.interrupt();
                    let _ = transport
                        .emit(&ConversationEvent::Error {
                            message: "interrupted".to_owned(),
                            recoverable: Some(true),
                        })
                        .await;
                }
                ControlMessage::SetSystemPrompt { text } => {
                    self.config.system_prompt = Some(text);
                    let _ = transport
                        .emit(&ConversationEvent::StatusUpdate {
                            text: "system_prompt updated".to_owned(),
                        })
                        .await;
                }
                ControlMessage::Ping => {
                    let _ = transport.emit(&ConversationEvent::Heartbeat).await;
                }
            }
        }
    }

    // ── Interactive loop (human-facing) ──────────────────────────────────────

    /// Read plain-text lines from stdin and run turns until EOF or `quit`.
    ///
    /// Mirrors `AgentRunner::run_interactive_loop`.
    pub async fn run_interactive_loop<T: Transport>(&mut self, transport: &mut T) {
        use tokio::io::AsyncWriteExt as _;

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();
        let ctx = ChainContext::default();

        let banner = format!(
            "Light Architects agent — cwd: {}\nType 'quit' or Ctrl-D to exit.\n",
            self.config.cwd.display()
        );
        let _ = stdout.write_all(banner.as_bytes()).await;
        let _ = stdout.flush().await;

        loop {
            let _ = stdout.write_all(b"> ").await;
            let _ = stdout.flush().await;

            let Ok(Some(line)) = lines.next_line().await else {
                break;
            };
            let input = line.trim();
            if input.is_empty() {
                continue;
            }
            if input.eq_ignore_ascii_case("quit") || input.eq_ignore_ascii_case("exit") {
                break;
            }

            self.clear_interrupt();
            if let Err(e) = self.run_turn(input, transport, &ctx).await {
                let _ = transport
                    .emit(&ConversationEvent::Error {
                        message: e.to_string(),
                        recoverable: Some(true),
                    })
                    .await;
            }
        }
    }
}

// ── ControlMessage ────────────────────────────────────────────────────────────

/// Inbound control messages for the NDJSON session loop.
///
/// SDK-native counterpart of the gateway's `ControlMessage`; kept lean —
/// only the variants the session loop handles are included.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ControlMessage {
    /// Begin a new agent turn with the given user text.
    SendMessage {
        /// User message content.
        text: String,
    },
    /// Cancel the in-flight turn.
    Interrupt,
    /// Replace the session system prompt (takes effect on the next turn).
    SetSystemPrompt {
        /// New system prompt text.
        text: String,
    },
    /// Keepalive ping.
    Ping,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::agent::provider::{SchemaMode, TokenUsage};
    use crate::agent::{AgentResponse, ProviderCapabilities, ProviderError};

    // Minimal no-op provider for unit tests.
    struct EchoProvider;

    #[async_trait::async_trait]
    impl LlmAgentProvider for EchoProvider {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                schema_enforcement: SchemaMode::None,
                native_budget_cap: false,
                native_turn_cap: false,
                auth_inherits_session: true,
            }
        }

        fn estimate_cost(&self, _input: u32, _output: u32) -> f64 {
            0.0
        }

        async fn spawn(
            &self,
            req: crate::agent::SanitizedAgentRequest,
        ) -> Result<AgentResponse, ProviderError> {
            Ok(AgentResponse {
                output: serde_json::Value::String(format!("echo: {}", req.safe_prompt())),
                turns_used: 1,
                cost_usd: 0.0,
                tokens: TokenUsage {
                    input: 10,
                    output: 5,
                },
                provider_attrs: std::collections::HashMap::new(),
                retry_count: 0,
            })
        }
    }

    #[tokio::test]
    async fn run_turn_stores_memory_and_emits_events() {
        let config = SessionConfig::default();
        let mut session = ConversationSession::new(config, Arc::new(EchoProvider));
        let mut buf = Vec::new();
        let mut transport = crate::agent::conversation::NdjsonTransport::new(&mut buf);

        let result = session
            .run_turn("hello", &mut transport, &ChainContext::default())
            .await;
        assert!(result.is_ok());
        assert_eq!(session.state.turn_count, 1);
        assert_eq!(session.memory.turn_count(), 2); // user + assistant

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("echo: hello"));
    }

    #[tokio::test]
    async fn interrupt_before_turn_returns_error() {
        let config = SessionConfig::default();
        let mut session = ConversationSession::new(config, Arc::new(EchoProvider));
        session.interrupt();

        let mut buf = Vec::new();
        let mut transport = crate::agent::conversation::NdjsonTransport::new(&mut buf);
        let result = session
            .run_turn("hello", &mut transport, &ChainContext::default())
            .await;
        assert!(matches!(result, Err(SessionError::Interrupted)));
        assert_eq!(session.state.turn_count, 0);
    }

    #[tokio::test]
    async fn clear_interrupt_allows_next_turn() {
        let config = SessionConfig::default();
        let mut session = ConversationSession::new(config, Arc::new(EchoProvider));
        session.interrupt();
        session.clear_interrupt();

        let mut buf = Vec::new();
        let mut transport = crate::agent::conversation::NdjsonTransport::new(&mut buf);
        let result = session
            .run_turn("hi", &mut transport, &ChainContext::default())
            .await;
        assert!(result.is_ok());
    }
}
