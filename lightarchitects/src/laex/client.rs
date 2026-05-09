//! [`LaexClient`] — typed client for LÆX's `laexTools` gateway-dispatched orchestrator.

use std::time::Duration;

use serde_json::Value;

use crate::core::constants::DEFAULT_TIMEOUT_SECS;
use crate::core::error::SdkError;
use crate::core::transport::Transport;
use crate::core::{McpClient, RetryConfig, SiblingId};

use crate::laex::content::{unwrap_json, unwrap_text};
use crate::laex::types::{
    ActionOutput, CanonCheckResult, CanonEvaluateResult, EffectivenessScoreResult, GovernanceLayer,
    LayerReviewResult, MatrixRatifyResult, QueryCanonDriftResult, ReflectResult,
    RegisterDecisionResult,
};

/// Single virtual MCP tool name exposed by the gateway's inline LÆX handler.
const LAEX_TOOL: &str = "laexTools";

// ── LaexClient ─────────────────────────────────────────────────────────────────

/// Typed client for LÆX's `laexTools` gateway-dispatched orchestrator (9 routable
/// actions).
///
/// Routable actions: `canon_check`, `canon_evaluate`, `matrix_ratify`,
/// `effectiveness_score`, `reflect`, `layer1_review`, `layer2_review`,
/// `layer3_review`, `layer4_review`.
///
/// Two call paths:
///
/// - **Typed methods** — one method per routable action with structured returns.
///   Use when the action is known at compile time.
/// - **Generic adapter** — [`LaexClient::action`] accepts any action name and
///   raw JSON params. Use for dynamic dispatch or higher-level orchestration.
///
/// LÆX is **inline-only** (gateway-dispatched, no standalone stdio binary).
/// Construct via [`LaexClient::builder`] only — no `local_builder` exists.
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), lightarchitects::core::SdkError> {
/// use lightarchitects::laex::LaexClient;
///
/// let client = LaexClient::builder().api_key("la_your_key_here").build()?;
///
/// let check = client.canon_check("ship hot-fix without test", false).await?;
/// println!("{}", check.framework);
/// # Ok(()) }
/// ```
pub struct LaexClient<T: Transport> {
    pub(crate) inner: McpClient<T>,
}

impl<T: Transport> LaexClient<T> {
    /// Construct a client from an already-connected transport.
    ///
    /// Intended for testing — pass a mock transport to exercise all methods
    /// without going through the gateway.
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

    /// Call any LÆX action by name with raw JSON parameters.
    ///
    /// Routes through the gateway's `laexTools` virtual orchestrator. Prefer
    /// the typed methods (`canon_check`, `effectiveness_score`, …) for
    /// compile-time parameter safety.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns `isError: true`.
    pub async fn action(&self, action: &str, params: Value) -> Result<ActionOutput, SdkError> {
        let wrapped = serde_json::json!({ "action": action, "params": params });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw, action)?,
        })
    }

    // ── Typed action methods ───────────────────────────────────────────────────

    /// Consult the canon registry for `decision` via LÆX's `canon_check` action.
    ///
    /// Wraps `core_tools/canon_check.rs::run`. Returns canonical-context headers
    /// the model can self-evaluate against.
    ///
    /// `verbose=true` includes full canon excerpts in the response.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn canon_check(
        &self,
        decision: &str,
        verbose: bool,
    ) -> Result<CanonCheckResult, SdkError> {
        let p = serde_json::json!({ "decision": decision, "verbose": verbose });
        let wrapped = serde_json::json!({ "action": "canon_check", "params": p });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "canon_check")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Build the 5-criteria evaluation framework for `candidate` via LÆX's
    /// `canon_evaluate` action.
    ///
    /// Wraps `core_tools/canon_evaluate.rs::run`. Returns `convergent_evidence`,
    /// `biblical_grounding`, `decision_shaping`, `pressure_tested`, `kevin_ratifies`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn canon_evaluate(&self, candidate: &str) -> Result<CanonEvaluateResult, SdkError> {
        let p = serde_json::json!({ "candidate": candidate });
        let wrapped = serde_json::json!({ "action": "canon_evaluate", "params": p });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "canon_evaluate")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Run the 4-layer governance audit over `manifest_path` via LÆX's
    /// `matrix_ratify` action.
    ///
    /// Returns a per-layer verdict map plus the overall ratification.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn matrix_ratify(&self, manifest_path: &str) -> Result<MatrixRatifyResult, SdkError> {
        let p = serde_json::json!({ "manifest_path": manifest_path });
        let wrapped = serde_json::json!({ "action": "matrix_ratify", "params": p });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "matrix_ratify")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Score a plan against `lasdlc-effectiveness-rubric.md` via LÆX's
    /// `effectiveness_score` action.
    ///
    /// `plan_id` is the plan codename or canonical id.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn effectiveness_score(
        &self,
        plan_id: &str,
    ) -> Result<EffectivenessScoreResult, SdkError> {
        let p = serde_json::json!({ "plan_id": plan_id });
        let wrapped = serde_json::json!({ "action": "effectiveness_score", "params": p });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "effectiveness_score")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Run the retrospective canon-evaluation ritual via LÆX's `reflect` action.
    ///
    /// `scope` describes what is being reflected on (e.g. build codename, time
    /// window). `evidence` is optional supporting context.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn reflect(
        &self,
        scope: &str,
        evidence: Option<&str>,
    ) -> Result<ReflectResult, SdkError> {
        let mut p = serde_json::json!({ "scope": scope });
        if let Some(ev) = evidence {
            p["evidence"] = Value::String(ev.to_owned());
        }
        let wrapped = serde_json::json!({ "action": "reflect", "params": p });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "reflect")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Run a single-layer governance review via LÆX's `layer{1-4}_review` action.
    ///
    /// `layer` selects which of the 4 layers (Security / Methodology / Product /
    /// Ethics). `target` is the manifest path or build codename being reviewed.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn layer_review(
        &self,
        layer: GovernanceLayer,
        target: &str,
    ) -> Result<LayerReviewResult, SdkError> {
        let action_name = match layer {
            GovernanceLayer::Security => "layer1_review",
            GovernanceLayer::Methodology => "layer2_review",
            GovernanceLayer::Product => "layer3_review",
            GovernanceLayer::Ethics => "layer4_review",
        };
        let p = serde_json::json!({ "target": target, "layer": layer.as_str() });
        let wrapped = serde_json::json!({ "action": action_name, "params": p });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, action_name)?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    // ── Internal action methods (gateway-internal only) ───────────────────────

    /// Append a ratification record to the canon decision-registry.
    ///
    /// Internal action — not gateway-routed via `auto_route`. Callers must invoke
    /// this method directly when they hold an in-process [`LaexClient`] handle.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn register_decision(
        &self,
        decision: &str,
        ratifier: &str,
    ) -> Result<RegisterDecisionResult, SdkError> {
        let p = serde_json::json!({ "decision": decision, "ratifier": ratifier });
        let wrapped = serde_json::json!({ "action": "register_decision", "params": p });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "register_decision")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }

    /// Compute drift between the local canon registry and the platform helix.
    ///
    /// Internal action — not gateway-routed via `auto_route`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or LÆX returns an error.
    pub async fn query_canon_drift(&self) -> Result<QueryCanonDriftResult, SdkError> {
        let wrapped = serde_json::json!({ "action": "query_canon_drift", "params": {} });
        let raw = self.inner.call_tool(LAEX_TOOL, wrapped).await?;
        let json = unwrap_json(raw, "query_canon_drift")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }
}

// ── HttpTransport-backed builder ──────────────────────────────────────────────

#[cfg(feature = "http-client")]
impl LaexClient<crate::core::HttpTransport> {
    /// Construct a builder for the HTTP-backed [`LaexClient`].
    #[must_use]
    pub fn builder() -> LaexClientBuilder {
        LaexClientBuilder::default()
    }
}

/// Builder for [`LaexClient`] backed by the Light Architects cloud API.
///
/// LÆX is **inline-only** — there is no `local_builder` because LÆX has no
/// standalone stdio binary; all calls route through the gateway's inline
/// `LaexHandler`.
///
/// ```no_run
/// # fn example() -> Result<(), lightarchitects::core::SdkError> {
/// use lightarchitects::laex::LaexClient;
///
/// let client = LaexClient::builder()
///     .api_key("la_your_key_here")
///     .build()?;
/// # Ok(()) }
/// ```
#[cfg(feature = "http-client")]
pub struct LaexClientBuilder {
    api_key: String,
    base_url: String,
    timeout: Duration,
    retry: RetryConfig,
}

#[cfg(feature = "http-client")]
impl Default for LaexClientBuilder {
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
impl LaexClientBuilder {
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

    /// Build the [`LaexClient`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if the API key is empty or the HTTP
    /// client cannot be constructed.
    pub fn build(self) -> Result<LaexClient<crate::core::HttpTransport>, SdkError> {
        let transport = crate::core::HttpTransport::builder(SiblingId::Laex)
            .api_key(self.api_key)
            .base_url(self.base_url)
            .timeout(self.timeout)
            .build()?;
        Ok(LaexClient::from_transport(transport, self.retry))
    }
}
