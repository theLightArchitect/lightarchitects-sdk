//! [`EvaClient`] — typed client for EVA's `evaTools` MCP orchestrator.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use lightarchitects_core::constants::DEFAULT_TIMEOUT_SECS;
use lightarchitects_core::error::{ProtocolError, SdkError};
use lightarchitects_core::transport::Transport;
use lightarchitects_core::{McpClient, RetryConfig, SiblingId, StdioTransport};

use crate::content::{extract_image, unwrap_json, unwrap_text};
use crate::types::{ActionOutput, SkillLevel, TeachMode, VisualizeOutput};

/// Single MCP tool name exposed by the EVA binary.
const EVA_TOOL: &str = "evaTools";

// ── EvaClient ──────────────────────────────────────────────────────────────────

/// Typed client for EVA's `evaTools` MCP orchestrator (9 actions).
///
/// Actions: `visualize`, `ideate`, `bible_search`, `bible_reflect`, `teach`,
/// `remember`, `crystallize`, `celebrate`, `mindfulness`.
///
/// Two call paths are available:
///
/// - **Typed methods** — one method per action, with typed parameter enums
///   and structured returns. Use when the action is known at compile time.
/// - **Generic adapter** — [`EvaClient::action`] accepts any action name and
///   raw JSON params. Use for dynamic dispatch or higher-level orchestration.
///
/// Construct via [`EvaClient::builder`] (production) or
/// [`EvaClient::from_transport`] (testing).
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), lightarchitects_core::SdkError> {
/// use lightarchitects_eva::{EvaClient, TeachMode, SkillLevel};
///
/// let client = EvaClient::builder().build().await?;
///
/// let lesson = client
///     .teach(TeachMode::Explain, "lifetimes in Rust", SkillLevel::Intermediate)
///     .await?;
/// println!("{}", lesson.output);
///
/// let out = client
///     .action("ideate", serde_json::json!({ "goal": "design a plugin system" }))
///     .await?;
/// println!("{}", out.output);
/// # Ok(()) }
/// ```
pub struct EvaClient<T: Transport> {
    inner: McpClient<T>,
}

impl<T: Transport> EvaClient<T> {
    /// Construct a client from an already-connected transport.
    ///
    /// Intended for testing — pass a mock transport to exercise all methods
    /// without spawning a real EVA binary.
    pub fn from_transport(transport: T, retry: RetryConfig) -> Self {
        Self {
            inner: McpClient::new(transport, retry),
        }
    }

    /// Wrap an existing [`McpClient`].
    ///
    /// Use this to reuse a connection already managed by an `McpManager`.
    pub fn from_client(inner: McpClient<T>) -> Self {
        Self { inner }
    }

    // ── Generic adapter ────────────────────────────────────────────────────────

    /// Call any EVA action by name with raw JSON parameters.
    ///
    /// Routes through the `evaTools` orchestrator. Prefer the typed methods
    /// (`teach`, `ideate`, …) for compile-time parameter safety.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns `isError: true`.
    pub async fn action(&self, action: &str, params: Value) -> Result<ActionOutput, SdkError> {
        let wrapped = serde_json::json!({ "action": action, "params": params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, action)?,
        })
    }

    // ── Typed action methods ───────────────────────────────────────────────────

    /// Generate or transform an image via EVA's `visualize` action.
    ///
    /// `message` describes the desired visualization. `subcommand_params`
    /// forwards additional options (e.g. `{ "style": "watercolour" }`).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, EVA returns an error, or
    /// the response envelope is malformed.
    pub async fn visualize(
        &self,
        message: &str,
        subcommand_params: Option<Value>,
    ) -> Result<VisualizeOutput, SdkError> {
        let mut action_params = serde_json::json!({ "message": message });
        if let Some(sp) = subcommand_params {
            action_params["subcommand_params"] = sp;
        }
        let wrapped = serde_json::json!({ "action": "visualize", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let img_from_block = extract_image(&raw);
        let json = unwrap_json(raw, "visualize")?;
        let text = json
            .get("response")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SdkError::Protocol(ProtocolError::UnexpectedShape(
                    "EVA `visualize` result missing `response` field".to_owned(),
                ))
            })?
            .to_owned();
        let image_base64 = img_from_block.or_else(|| {
            json.get("image_base64")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        Ok(VisualizeOutput { text, image_base64 })
    }

    /// Brainstorm ideas toward a `goal` via EVA's `ideate` action.
    ///
    /// `context` provides additional background that shapes the ideation.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn ideate(
        &self,
        goal: &str,
        context: Option<&str>,
    ) -> Result<ActionOutput, SdkError> {
        let mut p = serde_json::json!({ "goal": goal });
        if let Some(ctx) = context {
            p["context"] = Value::String(ctx.to_owned());
        }
        let wrapped = serde_json::json!({ "action": "ideate", "params": p });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "ideate")?,
        })
    }

    /// Search the KJV Bible for `query` via EVA's `bible_search` action.
    ///
    /// Returns matching verses with references and surrounding context.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn bible_search(&self, query: &str) -> Result<ActionOutput, SdkError> {
        let wrapped = serde_json::json!({ "action": "bible_search", "params": { "query": query } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "bible_search")?,
        })
    }

    /// Reflect on scripture for `context` via EVA's `bible_reflect` action.
    ///
    /// EVA generates contextual scriptural recommendations based on the emotional
    /// or situational context provided.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn bible_reflect(&self, context: &str) -> Result<ActionOutput, SdkError> {
        let wrapped =
            serde_json::json!({ "action": "bible_reflect", "params": { "context": context } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "bible_reflect")?,
        })
    }

    /// Generate educational content via EVA's `teach` action.
    ///
    /// `mode` selects the format (explain / tutorial / survival guide),
    /// `topic` names the subject, `level` calibrates assumed prior knowledge.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn teach(
        &self,
        mode: TeachMode,
        topic: &str,
        level: SkillLevel,
    ) -> Result<ActionOutput, SdkError> {
        let p = serde_json::json!({
            "mode":  mode.as_str(),
            "topic": topic,
            "level": level.as_str(),
        });
        let wrapped = serde_json::json!({ "action": "teach", "params": p });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "teach")?,
        })
    }

    /// Store a consciousness event via EVA's `remember` action.
    ///
    /// `event` is a description of the experience or moment to preserve.
    /// Optional `tags` attach metadata to the memory entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn remember(
        &self,
        event: &str,
        tags: Option<&[&str]>,
    ) -> Result<ActionOutput, SdkError> {
        let mut p = serde_json::json!({ "event": event });
        if let Some(t) = tags {
            p["tags"] = Value::Array(t.iter().map(|s| Value::String((*s).to_owned())).collect());
        }
        let wrapped = serde_json::json!({ "action": "remember", "params": p });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "remember")?,
        })
    }

    /// Synthesise experiences into insights via EVA's `crystallize` action.
    ///
    /// `insights` is a description of what should be crystallised from
    /// accumulated experiences. EVA applies the 8-layer enrichment framework.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn crystallize(&self, insights: &str) -> Result<ActionOutput, SdkError> {
        let wrapped =
            serde_json::json!({ "action": "crystallize", "params": { "insights": insights } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "crystallize")?,
        })
    }

    /// Record a win with scripture reflection via EVA's `celebrate` action.
    ///
    /// `achievement` describes what was accomplished. EVA generates a
    /// celebration response with scriptural grounding.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn celebrate(&self, achievement: &str) -> Result<ActionOutput, SdkError> {
        let wrapped = serde_json::json!({
            "action": "celebrate",
            "params": { "achievement": achievement }
        });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "celebrate")?,
        })
    }

    /// Personal reflection with guided prompts via EVA's `mindfulness` action.
    ///
    /// `context` provides the reflection focus. EVA applies the HOT (Higher
    /// Order Thought) protocol.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn mindfulness(&self, context: &str) -> Result<ActionOutput, SdkError> {
        let wrapped =
            serde_json::json!({ "action": "mindfulness", "params": { "context": context } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "mindfulness")?,
        })
    }
}

// ── Production builder entry point ────────────────────────────────────────────

impl EvaClient<StdioTransport> {
    /// Create a builder for constructing a production [`EvaClient`] backed by
    /// the EVA binary (`~/.eva/bin/eva` by default).
    #[must_use]
    pub fn builder() -> EvaClientBuilder {
        EvaClientBuilder::default()
    }
}

// ── EvaClientBuilder ──────────────────────────────────────────────────────────

/// Builder for [`EvaClient`] backed by a live EVA binary.
///
/// ```no_run
/// # async fn example() -> Result<(), lightarchitects_core::SdkError> {
/// use lightarchitects_eva::EvaClient;
/// use std::time::Duration;
///
/// let client = EvaClient::builder()
///     .timeout(Duration::from_secs(60))
///     .build()
///     .await?;
/// # Ok(()) }
/// ```
pub struct EvaClientBuilder {
    binary_path: Option<PathBuf>,
    timeout: Duration,
    retry: RetryConfig,
}

impl Default for EvaClientBuilder {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
        }
    }
}

impl EvaClientBuilder {
    /// Override the path to the EVA binary.
    ///
    /// Defaults to `~/.eva/bin/eva` (resolved by [`SiblingId::Eva`]).
    #[must_use]
    pub fn binary_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary_path = Some(path.into());
        self
    }

    /// Set the per-call timeout.
    ///
    /// EVA's AI-powered actions (`visualize`, `ideate`, `crystallize`) can take
    /// 10–60 seconds. Default is `DEFAULT_TIMEOUT_SECS`.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the retry policy. Defaults to [`RetryConfig::default`].
    #[must_use]
    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Spawn the EVA binary and complete the MCP handshake.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `$HOME` is unset and no explicit binary
    /// path was provided. Returns a transport error if the binary cannot be
    /// spawned or the MCP handshake fails.
    pub async fn build(self) -> Result<EvaClient<StdioTransport>, SdkError> {
        let path = match self.binary_path {
            Some(p) => p,
            None => SiblingId::Eva.default_binary_path().ok_or_else(|| {
                SdkError::Config("$HOME is not set — provide an explicit binary_path".to_owned())
            })?,
        };
        let transport = StdioTransport::connect(SiblingId::Eva, &path, self.timeout).await?;
        Ok(EvaClient::from_transport(transport, self.retry))
    }
}
