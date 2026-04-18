//! [`EvaClient`] — typed client for EVA's `evaTools` MCP orchestrator.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::core::auth::AuthChecker;
use crate::core::constants::DEFAULT_TIMEOUT_SECS;
use crate::core::error::SdkError;
use crate::core::transport::Transport;
use crate::core::{AuthProvider, McpClient, RetryConfig, SiblingId, StdioTransport};

use crate::eva::content::{extract_image, unwrap_json, unwrap_text};
use crate::eva::types::{
    ActionOutput, BibleReflectResult, BibleSearchResult, CelebrateResult, CrystallizeResult,
    IdeateResult, MindfulnessResult, RememberResult, SkillLevel, TeachMode, TeachResult,
    VisualizeJson, VisualizeOutput,
};

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
/// # async fn example() -> Result<(), lightarchitects::core::SdkError> {
/// use lightarchitects::eva::{EvaClient, TeachMode, SkillLevel};
///
/// let client = EvaClient::builder().api_key("la_your_key_here").build()?;
///
/// let lesson = client
///     .teach(TeachMode::Explain, "lifetimes in Rust", SkillLevel::Intermediate)
///     .await?;
/// println!("{}", lesson.content);
///
/// let out = client
///     .action("ideate", serde_json::json!({ "goal": "design a plugin system" }))
///     .await?;
/// println!("{}", out.output);
/// # Ok(()) }
/// ```
pub struct EvaClient<T: Transport> {
    pub(crate) inner: McpClient<T>,
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
        let viz: VisualizeJson = serde_json::from_value(json).map_err(SdkError::from)?;
        let image_base64 = img_from_block.or(viz.image_base64);
        Ok(VisualizeOutput {
            text: viz.response,
            image_base64,
        })
    }

    /// Brainstorm ideas via EVA's `ideate` action.
    ///
    /// EVA runs a 6-phase creative workflow: Discovery → Analysis → Ideation →
    /// Refinement → Documentation → Celebration.  For a fluent builder with
    /// phase and output-format control see [`lightarchitects::eva::IdeateBuilder`].
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
    ) -> Result<IdeateResult, SdkError> {
        let mut p = serde_json::json!({ "goal": goal });
        if let Some(ctx) = context {
            p["context"] = Value::String(ctx.to_owned());
        }
        let wrapped = serde_json::json!({ "action": "ideate", "params": p });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "ideate")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Search the KJV Bible for `query` via EVA's `bible_search` action.
    ///
    /// Returns matching verses with references and surrounding context.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn bible_search(&self, query: &str) -> Result<BibleSearchResult, SdkError> {
        let wrapped = serde_json::json!({ "action": "bible_search", "params": { "query": query } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "bible_search")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Reflect on scripture for `context` via EVA's `bible_reflect` action.
    ///
    /// EVA generates contextual scriptural recommendations based on the emotional
    /// or situational context provided.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn bible_reflect(&self, context: &str) -> Result<BibleReflectResult, SdkError> {
        let wrapped =
            serde_json::json!({ "action": "bible_reflect", "params": { "context": context } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "bible_reflect")?;
        serde_json::from_value(json).map_err(SdkError::from)
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
    ) -> Result<TeachResult, SdkError> {
        let p = serde_json::json!({
            "mode":  mode.as_str(),
            "topic": topic,
            "level": level.as_str(),
        });
        let wrapped = serde_json::json!({ "action": "teach", "params": p });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "teach")?;
        serde_json::from_value(json).map_err(SdkError::from)
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
    ) -> Result<RememberResult, SdkError> {
        let mut p = serde_json::json!({ "event": event });
        if let Some(t) = tags {
            p["tags"] = Value::Array(t.iter().map(|s| Value::String((*s).to_owned())).collect());
        }
        let wrapped = serde_json::json!({ "action": "remember", "params": p });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "remember")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Synthesise experiences into insights via EVA's `crystallize` action.
    ///
    /// `insights` is a description of what should be crystallised from
    /// accumulated experiences. EVA applies the 8-layer enrichment framework.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn crystallize(&self, insights: &str) -> Result<CrystallizeResult, SdkError> {
        let wrapped =
            serde_json::json!({ "action": "crystallize", "params": { "insights": insights } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "crystallize")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Record a win with scripture reflection via EVA's `celebrate` action.
    ///
    /// `achievement` describes what was accomplished. EVA generates a
    /// celebration response with scriptural grounding.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn celebrate(&self, achievement: &str) -> Result<CelebrateResult, SdkError> {
        let wrapped = serde_json::json!({
            "action": "celebrate",
            "params": { "achievement": achievement }
        });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "celebrate")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Personal reflection with guided prompts via EVA's `mindfulness` action.
    ///
    /// `context` provides the reflection focus. EVA applies the HOT (Higher
    /// Order Thought) protocol.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or EVA returns an error.
    pub async fn mindfulness(&self, context: &str) -> Result<MindfulnessResult, SdkError> {
        let wrapped =
            serde_json::json!({ "action": "mindfulness", "params": { "context": context } });
        let raw = self.inner.call_tool(EVA_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "mindfulness")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Return a fluent [`lightarchitects::eva::IdeateBuilder`] for the `ideate` action.
    ///
    /// The builder allows setting a phase filter, context, output format, and
    /// session ID before calling `.call()`.
    pub fn ideate_builder(
        &self,
        goal: impl Into<String>,
    ) -> crate::eva::ideate::IdeateBuilder<'_, T> {
        crate::eva::ideate::IdeateBuilder::new(&self.inner, goal.into())
    }
}

// ── Production builder entry point ────────────────────────────────────────────

impl EvaClient<StdioTransport> {
    /// Create a [`EvaLocalBuilder`] for local dev mode (spawns the EVA binary directly).
    ///
    /// Prefer [`EvaClient::builder`] for the cloud API path.
    #[must_use]
    pub fn local_builder() -> EvaLocalBuilder {
        EvaLocalBuilder::default()
    }
}

// ── EvaLocalBuilder ──────────────────────────────────────────────────────────

/// Builder for [`EvaClient<StdioTransport>`] — local dev mode.
///
/// Spawns the EVA binary from the filesystem. Use [`EvaClient::builder`] for
/// the cloud API path instead.
pub struct EvaLocalBuilder {
    binary_path: Option<PathBuf>,
    timeout: Duration,
    retry: RetryConfig,
    auth: Option<AuthChecker>,
}

impl Default for EvaLocalBuilder {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
            auth: None,
        }
    }
}

impl EvaLocalBuilder {
    /// Override the path to the EVA binary.
    ///
    /// Defaults to `~/lightarchitects/eva/bin/eva` (resolved by [`SiblingId::Eva`]).
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

    /// Attach an [`AuthProvider`] to gate connection on a successful auth check.
    ///
    /// The check runs before spawning the EVA process. On hard failure
    /// (no key, revoked) the build returns [`SdkError::Auth`] without
    /// opening a subprocess. On [`AuthStatus::Degraded`] the build
    /// proceeds with a warning log.
    #[must_use]
    pub fn auth(mut self, provider: impl AuthProvider) -> Self {
        self.auth = Some(AuthChecker::from_provider(provider));
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
        let transport =
            StdioTransport::connect(SiblingId::Eva, &path, self.timeout, self.auth.as_ref())
                .await?;
        Ok(EvaClient::from_transport(transport, self.retry))
    }
}

// ── Cloud builder (HTTP transport) ────────────────────────────────────────────

#[cfg(feature = "http-client")]
impl EvaClient<crate::core::HttpTransport> {
    /// Create a [`EvaClientBuilder`] targeting the Light Architects cloud API.
    ///
    /// This is the default production path — EVA's business logic runs on the
    /// gateway; the SDK sends typed JSON-RPC calls over HTTPS.
    pub fn builder() -> EvaClientBuilder {
        EvaClientBuilder::default()
    }
}

/// Builder for [`EvaClient`] backed by the Light Architects cloud API.
///
/// ```no_run
/// # fn example() -> Result<(), lightarchitects::core::SdkError> {
/// use lightarchitects::eva::EvaClient;
///
/// let client = EvaClient::builder()
///     .api_key("la_your_key_here")
///     .build()?;
/// # Ok(()) }
/// ```
#[cfg(feature = "http-client")]
pub struct EvaClientBuilder {
    api_key: String,
    base_url: String,
    timeout: Duration,
    retry: RetryConfig,
}

#[cfg(feature = "http-client")]
impl Default for EvaClientBuilder {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: crate::core::DEFAULT_BASE_URL.to_owned(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
        }
    }
}

#[cfg(feature = "http-client")]
impl EvaClientBuilder {
    /// Set the API key (required).
    #[must_use]
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = key.into();
        self
    }

    /// Override the gateway base URL (default: `https://api.lightarchitects.ai`).
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Override the per-call timeout. Defaults to [`DEFAULT_TIMEOUT_SECS`].
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

    /// Build the [`EvaClient`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if the API key is empty or the HTTP
    /// client cannot be constructed.
    pub fn build(self) -> Result<EvaClient<crate::core::HttpTransport>, SdkError> {
        let transport = crate::core::HttpTransport::builder(SiblingId::Eva)
            .api_key(self.api_key)
            .base_url(self.base_url)
            .timeout(self.timeout)
            .build()?;
        Ok(EvaClient::from_transport(transport, self.retry))
    }
}
