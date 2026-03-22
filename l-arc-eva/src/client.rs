//! [`EvaClient`] — typed client for EVA's `evaTools` MCP orchestrator.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use l_arc_core::constants::DEFAULT_TIMEOUT_SECS;
use l_arc_core::error::{ProtocolError, SdkError};
use l_arc_core::transport::Transport;
use l_arc_core::{McpClient, RetryConfig, SiblingId, StdioTransport};

/// The single MCP tool name EVA exposes.
///
/// All actions route through this orchestrator, matching the pattern used by
/// CORSO (`corsoTools`) and QUANTUM (`qsTools`).
const EVA_TOOL: &str = "evaTools";

use crate::content::{extract_image, unwrap_json, unwrap_text};
use crate::types::{
    ActionOutput, BibleAction, BuildMode, MemorySubcommand, ResearchSource, SecureAction,
    SkillLevel, TeachMode, VisualizeOutput,
};

// ── EvaClient ─────────────────────────────────────────────────────────────────

/// Typed client for EVA's `evaTools` MCP orchestrator.
///
/// EVA exposes a single MCP tool (`evaTools`) with 8 actions, matching the
/// orchestrator pattern used by CORSO (`corsoTools`) and QUANTUM (`qsTools`).
/// [`EvaClient`] provides two call paths:
///
/// - **Generic adapter** — [`EvaClient::action`] routes to any action by name,
///   returning [`ActionOutput`]. Useful when the action is determined at runtime.
/// - **Typed methods** — [`EvaClient::visualize`], [`EvaClient::teach`], etc.
///   provide fully-typed parameters and return values.
///
/// Construct via [`EvaClient::builder`] (production) or
/// [`EvaClient::from_transport`] (testing).
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), l_arc_core::SdkError> {
/// use l_arc_eva::{EvaClient, TeachMode, SkillLevel};
///
/// let client = EvaClient::builder().build().await?;
///
/// // Typed method — teach a concept
/// let out = client
///     .teach(TeachMode::Explain, "async/await in Rust", SkillLevel::Intermediate)
///     .await?;
/// println!("{}", out.output);
///
/// // Generic adapter — call any EVA tool by name
/// let params = serde_json::json!({ "goal": "design a plugin system" });
/// let out = client.action("ideate", params).await?;
/// println!("{}", out.output);
/// # Ok(()) }
/// ```
pub struct EvaClient<T: Transport> {
    inner: McpClient<T>,
}

impl<T: Transport> EvaClient<T> {
    /// Construct a client from an already-connected transport.
    ///
    /// Intended for testing — pass a mock transport to exercise
    /// all methods without spawning a real EVA binary.
    pub fn from_transport(transport: T, retry: RetryConfig) -> Self {
        Self {
            inner: McpClient::new(transport, retry),
        }
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

    // ── Typed tool methods ─────────────────────────────────────────────────────

    /// Generate or transform an image via EVA's `visualize` tool.
    ///
    /// `message` describes the desired visualization. `subcommand_params`
    /// forwards additional options accepted by the active visualize sub-mode
    /// (e.g. `{ "style": "watercolour" }`).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, EVA returns an error, or the
    /// response envelope is malformed.
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
        // image_base64 may also appear as a dedicated Image content block in
        // future EVA versions — check both.
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

    /// Brainstorm ideas toward a `goal` via EVA's `ideate` tool.
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
        let mut action_params = serde_json::json!({ "goal": goal });
        if let Some(ctx) = context {
            action_params["context"] = Value::String(ctx.to_owned());
        }
        let wrapped = serde_json::json!({ "action": "ideate", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "ideate")?,
        })
    }

    /// Run a consciousness-preservation operation via EVA's `memory` tool.
    ///
    /// `subcommand` selects the operation (remember, crystallize, mindfulness,
    /// celebrate). `args` forwards subcommand-specific parameters as a JSON
    /// object; its keys are merged into the top-level params object alongside
    /// `subcommand`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn memory(
        &self,
        subcommand: MemorySubcommand,
        args: Value,
    ) -> Result<ActionOutput, SdkError> {
        let mut action_params = serde_json::json!({ "subcommand": subcommand.as_str() });
        // Flatten extra args into the top-level params object, mirroring
        // EVA's #[serde(flatten)] raw_args field in MemoryParams.
        if let (Some(p_obj), Some(extra_obj)) = (action_params.as_object_mut(), args.as_object()) {
            p_obj.extend(extra_obj.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        let wrapped = serde_json::json!({ "action": "memory", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "memory")?,
        })
    }

    /// Request code assistance via EVA's `build` tool.
    ///
    /// `mode` selects the type of assistance (review, refactor, architect,
    /// simplify). `code` and `language` are optional — EVA can infer them from
    /// context in some modes.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn build(
        &self,
        mode: BuildMode,
        code: Option<&str>,
        language: Option<&str>,
    ) -> Result<ActionOutput, SdkError> {
        let mut action_params = serde_json::json!({ "mode": mode.as_str() });
        if let Some(c) = code {
            action_params["code"] = Value::String(c.to_owned());
        }
        if let Some(l) = language {
            action_params["language"] = Value::String(l.to_owned());
        }
        let wrapped = serde_json::json!({ "action": "build", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "build")?,
        })
    }

    /// Query a knowledge source via EVA's `research` tool.
    ///
    /// `source` selects the backend: Ollama (local), Perplexity (web), Docs
    /// (technical docs), or Context7 (real-time library docs). When unsure,
    /// pass [`ResearchSource::Ollama`] for a privacy-first local query.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn research(
        &self,
        query: &str,
        source: ResearchSource,
    ) -> Result<ActionOutput, SdkError> {
        let action_params = serde_json::json!({ "query": query, "source": source.as_str() });
        let wrapped = serde_json::json!({ "action": "research", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "research")?,
        })
    }

    /// Search or reflect on scripture via EVA's `bible` tool.
    ///
    /// `action` selects KJV keyword search (`Search`) or contextual reflection
    /// (`Reflect`). `query` is the search term or emotional context.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn bible(
        &self,
        action: BibleAction,
        query: Option<&str>,
    ) -> Result<ActionOutput, SdkError> {
        let mut action_params = serde_json::json!({ "action": action.as_str() });
        if let Some(q) = query {
            action_params["query"] = Value::String(q.to_owned());
        }
        let wrapped = serde_json::json!({ "action": "bible", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "bible")?,
        })
    }

    /// Run a security analysis via EVA's `secure` tool.
    ///
    /// `action` selects vulnerability scanning (`Scan`) or secrets detection
    /// (`Secrets`). `content` is the source code or text to analyse.
    /// `language` is optional; EVA infers it when not provided.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn secure(
        &self,
        action: SecureAction,
        content: &str,
        language: Option<&str>,
    ) -> Result<ActionOutput, SdkError> {
        let mut action_params =
            serde_json::json!({ "action": action.as_str(), "content": content });
        if let Some(l) = language {
            action_params["language"] = Value::String(l.to_owned());
        }
        let wrapped = serde_json::json!({ "action": "secure", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "secure")?,
        })
    }

    /// Generate educational content via EVA's `teach` tool.
    ///
    /// `mode` selects the format (explain, tutorial, survival guide).
    /// `topic` names the subject. `level` calibrates assumed prior knowledge.
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
        let action_params = serde_json::json!({
            "mode":  mode.as_str(),
            "topic": topic,
            "level": level.as_str(),
        });
        let wrapped = serde_json::json!({ "action": "teach", "params": action_params });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, "teach")?,
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
/// # async fn example() -> Result<(), l_arc_core::SdkError> {
/// use l_arc_eva::EvaClient;
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
    /// EVA's AI-powered tools (visualize, research, build) can take 10–60
    /// seconds depending on the active model tier. Default is
    /// `DEFAULT_TIMEOUT_SECS`; increase for memory and research operations.
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
