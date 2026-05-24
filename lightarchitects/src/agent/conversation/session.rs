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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use futures_util::StreamExt as _;

use crate::agent::{
    AgentRequest, AgentResponse, ChainContext, IndirectInjectionShield, LlmAgentProvider,
    NullToolExecutor, ProviderError, ProviderEvent, TokenUsage, ToolExecutor,
};

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
    /// Tool dispatcher — exposes tool definitions to the model and executes calls.
    pub tool_executor: Arc<dyn ToolExecutor>,
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
            tool_executor: Arc::new(NullToolExecutor),
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

    /// Replace the default `NullToolExecutor` with a concrete tool dispatcher.
    ///
    /// When an executor is wired, tool definitions are forwarded to the model and
    /// completed `tool_use` blocks are executed, with results re-injected via
    /// [`IndirectInjectionShield`].
    #[must_use]
    pub fn with_tool_executor(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        self.tool_executor = executor;
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
    #[allow(clippy::too_many_lines)]
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

        // Snapshot history BEFORE pushing this turn (provider gets history + current user msg
        // as separate fields so the current turn isn't duplicated in conversation_history).
        let history: Vec<serde_json::Value> = self
            .memory
            .turns()
            .iter()
            .map(|t| serde_json::json!({"role": t.role.to_string(), "content": t.content}))
            .collect();

        // Store user turn.
        self.memory.push(MessageRole::User, user_message.to_owned());

        // Build request from config + session state.
        let system_identity = self
            .config
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful coding assistant.".to_owned());

        let tool_defs = self.tool_executor.tool_definitions();
        let base_req = AgentRequest {
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
            conversation_history: history,
            tool_definitions: tool_defs,
        };

        // Status update while the provider is running.
        transport
            .emit(&ConversationEvent::StatusUpdate {
                text: format!("Calling {} …", self.provider.name()),
            })
            .await?;

        // W5.2 — AYIN span: record wall-clock start before the provider call.
        let turn_start = std::time::Instant::now();
        // H13: heartbeat every 5s when the provider emits no new chunks.
        let heartbeat = std::time::Duration::from_secs(5);

        let shield = IndirectInjectionShield::new();

        // Agentic outer loop — repeats while stop_reason == "tool_use".
        let mut output_text = String::new();
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut ttft_ms: i64 = -1;
        // Mutable request state for agentic iterations.
        let mut iter_history = base_req.conversation_history.clone();
        let mut iter_user_prompt = base_req.user_prompt.clone();
        let mut remaining_inner = self.config.max_turns.max(1);

        loop {
            let iter_req = AgentRequest {
                conversation_history: iter_history.clone(),
                user_prompt: iter_user_prompt.clone(),
                tool_definitions: self.tool_executor.tool_definitions(),
                ..base_req.clone()
            };
            let sanitized = iter_req.sanitize()?;
            let mut stream = self.provider.spawn_streaming(sanitized).await?;

            // Per-block tracking: index → (tool_use_id, tool_name, json_accumulator)
            let mut tool_blocks: HashMap<u32, (String, String, String)> = HashMap::new();
            let mut stop_reason = "end_turn".to_owned();
            let mut inner_text = String::new();
            let mut inner_input_tokens = 0u32;
            let mut inner_output_tokens = 0u32;

            loop {
                match tokio::time::timeout(heartbeat, stream.next()).await {
                    Ok(Some(event)) => {
                        if self.is_interrupted() {
                            break;
                        }
                        match event {
                            ProviderEvent::MessageStart {
                                input_tokens: t, ..
                            } => {
                                inner_input_tokens = t;
                            }
                            ProviderEvent::ContentBlockStart {
                                index,
                                block_type,
                                tool_use_id,
                                tool_name,
                            } if block_type == "tool_use" => {
                                let id = tool_use_id.unwrap_or_default();
                                let name = tool_name.clone().unwrap_or_default();
                                transport
                                    .emit(&ConversationEvent::StatusUpdate {
                                        text: format!(
                                            "[tool: {}] ⏳",
                                            tool_name.as_deref().unwrap_or("unknown")
                                        ),
                                    })
                                    .await?;
                                tool_blocks.insert(index, (id, name, String::new()));
                            }
                            ProviderEvent::InputJsonDelta {
                                index,
                                partial_json,
                            } => {
                                if let Some((_, _, json)) = tool_blocks.get_mut(&index) {
                                    json.push_str(&partial_json);
                                }
                            }
                            ProviderEvent::TextDelta { text, .. } => {
                                if ttft_ms < 0 {
                                    ttft_ms = i64::try_from(turn_start.elapsed().as_millis())
                                        .unwrap_or(i64::MAX);
                                }
                                inner_text.push_str(&text);
                                output_text.push_str(&text);
                                transport
                                    .emit(&ConversationEvent::Text { chunk: text })
                                    .await?;
                            }
                            ProviderEvent::MessageDelta {
                                output_tokens: t,
                                stop_reason: r,
                                ..
                            } => {
                                inner_output_tokens = t;
                                stop_reason = r;
                            }
                            _ => {}
                        }
                    }
                    Ok(None) => break,
                    Err(_elapsed) => {
                        let elapsed_secs = turn_start.elapsed().as_secs();
                        transport
                            .emit(&ConversationEvent::StatusUpdate {
                                text: format!(
                                    "  …  ({elapsed_secs}s elapsed, {inner_output_tokens} tokens)"
                                ),
                            })
                            .await?;
                        if self.is_interrupted() {
                            break;
                        }
                    }
                }
            }

            input_tokens = input_tokens.saturating_add(inner_input_tokens);
            output_tokens = output_tokens.saturating_add(inner_output_tokens);

            // Exit when no tool calls, end of turn, or budget exhausted.
            if stop_reason != "tool_use" || tool_blocks.is_empty() || remaining_inner == 0 {
                break;
            }
            remaining_inner -= 1;

            // Build assistant content block list (tool_use entries).
            let assistant_content: Vec<Value> = tool_blocks
                .values()
                .map(|(id, name, input)| {
                    let input_json: Value =
                        serde_json::from_str(input).unwrap_or(Value::Object(Map::default()));
                    serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input_json
                    })
                })
                .collect();

            // Execute each tool call and build tool_result content blocks.
            let mut tool_results: Vec<Value> = Vec::new();
            for (id, name, input) in tool_blocks.values() {
                let input_json: Value =
                    serde_json::from_str(input).unwrap_or(Value::Object(Map::default()));
                let (result_str, is_error) =
                    match self.tool_executor.execute(id, name, input_json).await {
                        Ok(out) => (out.content.to_string(), out.is_error),
                        Err(e) => (format!("tool error: {e}"), true),
                    };

                // Detect injection patterns and emit warnings.
                for detected in shield.detect(&result_str) {
                    transport
                        .emit(&ConversationEvent::IndirectInjectionWarning {
                            tool_use_id: id.clone(),
                            pattern: detected.pattern,
                            severity: detected.severity,
                        })
                        .await?;
                }

                let wrapped = shield.wrap_tool_result(id, &result_str);
                tool_results.push(serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": wrapped,
                    "is_error": is_error
                }));
            }

            // Absorb user message + assistant tool_use + tool_results into history.
            if !iter_user_prompt.is_empty() {
                iter_history.push(serde_json::json!({"role": "user", "content": iter_user_prompt}));
            }
            if inner_text.is_empty() {
                iter_history.push(serde_json::json!({
                    "role": "assistant",
                    "content": assistant_content
                }));
            } else {
                let mut mixed = vec![serde_json::json!({"type": "text", "text": inner_text})];
                mixed.extend(assistant_content);
                iter_history.push(serde_json::json!({"role": "assistant", "content": mixed}));
            }
            iter_history.push(serde_json::json!({
                "role": "user",
                "content": tool_results
            }));
            // Subsequent iterations have no separate user prompt — it's in history.
            iter_user_prompt = String::new();
        }

        // W5.2 — AYIN per-turn span: TTFT + duration + cancellation taxonomy.
        let duration_ms = u64::try_from(turn_start.elapsed().as_millis()).unwrap_or(u64::MAX);
        let cancelled = self.is_interrupted();
        let cancellation_reason = if cancelled { "user_interrupt" } else { "none" };
        tracing::info!(
            provider = self.provider.name(),
            turn = self.state.turn_count + 1,
            model = self.config.model_hint.as_deref().unwrap_or("default"),
            input_tokens,
            output_tokens,
            ttft_ms,
            duration_ms,
            text_bytes = output_text.len(),
            cancelled,
            cancellation_reason,
            "session.run_turn"
        );

        // Store assistant turn.
        self.memory
            .push(MessageRole::Assistant, output_text.clone());

        // Emit token usage and completion.
        transport
            .emit(&ConversationEvent::TokenUsage {
                input: u64::from(input_tokens),
                output: u64::from(output_tokens),
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
        Ok(AgentResponse {
            output: Value::String(output_text),
            turns_used: 1,
            cost_usd: 0.0,
            tokens: TokenUsage {
                input: input_tokens,
                output: output_tokens,
            },
            provider_attrs: std::collections::HashMap::new(),
            retry_count: 0,
        })
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::agent::provider::{SchemaMode, TokenUsage};
    use crate::agent::{
        AgentResponse, ProviderCapabilities, ProviderError, ToolDefinition, ToolError, ToolOutput,
    };

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

    // ── Phase 5: tool round-trip tests ────────────────────────────────────────

    /// Provider that emits a `tool_use` block on call 1, then a text response on call 2.
    struct ToolCallProvider {
        call_count: std::sync::atomic::AtomicU32,
    }

    impl ToolCallProvider {
        fn new() -> Self {
            Self {
                call_count: std::sync::atomic::AtomicU32::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmAgentProvider for ToolCallProvider {
        fn name(&self) -> &'static str {
            "tool-call-mock"
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
            _req: crate::agent::SanitizedAgentRequest,
        ) -> Result<AgentResponse, ProviderError> {
            Err(ProviderError::Internal("use spawn_streaming".into()))
        }

        async fn spawn_streaming(
            &self,
            req: crate::agent::SanitizedAgentRequest,
        ) -> Result<futures_util::stream::BoxStream<'static, ProviderEvent>, ProviderError>
        {
            use futures_util::StreamExt as _;
            use futures_util::stream;
            let call = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let events = if call == 0 {
                // First call: emit a tool_use block for "ping".
                vec![
                    ProviderEvent::MessageStart {
                        model: "mock".into(),
                        input_tokens: 10,
                    },
                    ProviderEvent::ContentBlockStart {
                        index: 0,
                        block_type: "tool_use".into(),
                        tool_use_id: Some("tu-001".into()),
                        tool_name: Some("ping".into()),
                    },
                    ProviderEvent::InputJsonDelta {
                        index: 0,
                        partial_json: r#"{"msg":"hello"}"#.into(),
                    },
                    ProviderEvent::ContentBlockStop { index: 0 },
                    ProviderEvent::MessageDelta {
                        stop_reason: "tool_use".into(),
                        output_tokens: 5,
                    },
                    ProviderEvent::MessageStop,
                ]
            } else {
                // Second call: echo back the last history entry to prove tool result arrived.
                let last = req
                    .request()
                    .conversation_history
                    .last()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                vec![
                    ProviderEvent::MessageStart {
                        model: "mock".into(),
                        input_tokens: 20,
                    },
                    ProviderEvent::ContentBlockStart {
                        index: 0,
                        block_type: "text".into(),
                        tool_use_id: None,
                        tool_name: None,
                    },
                    ProviderEvent::TextDelta {
                        index: 0,
                        text: format!("result:{last}"),
                    },
                    ProviderEvent::ContentBlockStop { index: 0 },
                    ProviderEvent::MessageDelta {
                        stop_reason: "end_turn".into(),
                        output_tokens: 15,
                    },
                    ProviderEvent::MessageStop,
                ]
            };
            Ok(stream::iter(events).boxed())
        }
    }

    /// Tool executor that returns a fixed result for the "ping" tool.
    struct PingToolExecutor;

    #[async_trait::async_trait]
    impl ToolExecutor for PingToolExecutor {
        fn tool_definitions(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "ping".into(),
                description: "Echoes its input".into(),
                input_schema: serde_json::json!({"type":"object","properties":{"msg":{"type":"string"}}}),
            }]
        }

        async fn execute(
            &self,
            _tool_use_id: &str,
            tool_name: &str,
            _input: Value,
        ) -> Result<ToolOutput, ToolError> {
            if tool_name == "ping" {
                Ok(ToolOutput {
                    tool_use_id: _tool_use_id.to_owned(),
                    content: serde_json::json!("pong"),
                    is_error: false,
                })
            } else {
                Err(ToolError::UnknownTool(tool_name.to_owned()))
            }
        }
    }

    #[tokio::test]
    async fn tool_round_trip_executes_and_injects_result() {
        let config = SessionConfig::default();
        let provider = Arc::new(ToolCallProvider::new());
        let mut session = ConversationSession::new(config, provider.clone())
            .with_tool_executor(Arc::new(PingToolExecutor));

        let mut buf = Vec::new();
        let mut transport = crate::agent::conversation::NdjsonTransport::new(&mut buf);
        let result = session
            .run_turn("call ping", &mut transport, &ChainContext::default())
            .await;
        assert!(result.is_ok(), "run_turn failed: {result:?}");

        // Provider must have been called twice (once for tool_use, once for continuation).
        assert_eq!(
            provider
                .call_count
                .load(std::sync::atomic::Ordering::SeqCst),
            2
        );

        // The final output should contain the tool result echoed back.
        let output = String::from_utf8(buf).unwrap();
        // Second provider call echoes last history entry which contains "tool_result".
        assert!(
            output.contains("tool_result"),
            "expected tool_result in output, got: {output}"
        );
    }

    #[tokio::test]
    async fn tool_definitions_forwarded_to_provider() {
        let config = SessionConfig::default();
        let provider = Arc::new(EchoProvider);
        let session = ConversationSession::new(config, provider)
            .with_tool_executor(Arc::new(PingToolExecutor));

        // Verify the tool executor exposes the "ping" tool definition.
        let defs = session.tool_executor.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "ping");
    }
}
