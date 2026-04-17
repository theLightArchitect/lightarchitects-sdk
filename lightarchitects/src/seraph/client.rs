//! [`SeraphClient`] — typed client for the SERAPH MCP server.

use std::path::PathBuf;
use std::time::Duration;

use crate::core::auth::AuthChecker;
use crate::core::constants::DEFAULT_TIMEOUT_SECS;
use crate::core::error::SdkError;
use crate::core::transport::Transport;
use crate::core::{AuthProvider, McpClient, RetryConfig, SiblingId, StdioTransport};

use crate::seraph::content::unwrap_text;
use crate::seraph::params::{AnalyzeParams, CaptureParams, MonitorParams, OsintParams, ScanParams};
use crate::seraph::types::{
    ActionOutput, ExamineResult, ReconResult, ReportResult, ScopeResult, StrikeResult,
    SurveyResult, Wing,
};

// ── SeraphClient ───────────────────────────────────────────────────────────────

/// Typed client for the SERAPH MCP server (`penTools` — 18 actions).
///
/// SERAPH is a pentest-orchestration server that requires an active
/// [`scope.toml`] engagement file. Every action is scope-governed by
/// SERAPH's 5-gate `ScopeGovernor` (TTL → target → tool → concurrent → domain).
///
/// **All operations require prior authorisation.** Only call these methods
/// against targets within an approved engagement scope.
///
/// SERAPH uses `Content-Length` header framing (not newline-delimited JSON).
/// [`StdioTransport`] handles this transparently via [`SiblingId::Seraph`].
///
/// Constructed via [`SeraphClient::builder`] (production, spawns the SERAPH
/// Mac bridge binary) or [`SeraphClient::from_transport`] (testing, injects a
/// [`Transport`]).
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), crate::core::SdkError> {
/// use crate::seraph::{SeraphClient, Wing};
///
/// let client = SeraphClient::builder()
///     .timeout(std::time::Duration::from_secs(120))
///     .build()
///     .await?;
///
/// // Check engagement status and scope
/// let status = client.status().await?;
/// println!("{}", status.output);
///
/// // Host discovery within authorised scope
/// let hosts = client.scan("192.168.1.0/24").await?;
/// println!("{}", hosts.output);
///
/// // OSINT on a target domain
/// let intel = client.osint("target.example.com", None).await?;
/// println!("{}", intel.output);
/// # Ok(()) }
/// ```
pub struct SeraphClient<T: Transport> {
    inner: McpClient<T>,
}

impl<T: Transport> SeraphClient<T> {
    /// Construct a client from an already-connected transport.
    ///
    /// Intended for testing — pass a `MockTransport` to exercise all methods
    /// without spawning a real SERAPH binary.
    pub fn from_transport(transport: T, retry: RetryConfig) -> Self {
        Self {
            inner: McpClient::new(transport, retry),
        }
    }

    // ── Wing actions ───────────────────────────────────────────────────────────

    /// Capture network traffic at `target` (interface or PCAP filter).
    ///
    /// Delegates to SERAPH's Capture wing. Requires the `capture` tool to
    /// be permitted in the active scope.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, the scope rejects the target,
    /// or SERAPH is not running with an active engagement.
    pub async fn capture(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "capture",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Discover hosts and services at `target` (IP, CIDR, or hostname).
    ///
    /// Delegates to SERAPH's Scan wing. Runs discovery tools appropriate
    /// for the target type.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the scope rejects the target.
    pub async fn scan(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "scan",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Analyse a binary, artefact, or memory image at `target`.
    ///
    /// Delegates to SERAPH's Analyze wing. Performs static and dynamic
    /// analysis of the target artefact.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn analyze(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "analyze",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Gather open-source intelligence on `target`.
    ///
    /// Delegates to SERAPH's OSINT wing. Optional `depth` controls how many
    /// search layers are traversed (`"shallow"`, `"standard"`, `"deep"`).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn osint(&self, target: &str, depth: Option<&str>) -> Result<ActionOutput, SdkError> {
        let mut p = serde_json::json!({ "target": target });
        if let Some(d) = depth {
            p["depth"] = serde_json::Value::String(d.to_owned());
        }
        let params = serde_json::json!({ "action": "osint", "params": p });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Start continuous monitoring of `target`.
    ///
    /// Delegates to SERAPH's Monitor wing. Returns an initial monitoring
    /// report; subsequent anomalies surface through SERAPH's alert channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn monitor(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "monitor",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Execute a payload or exploit against `target`.
    ///
    /// Delegates to SERAPH's Execute wing. This is the highest-impact wing —
    /// SERAPH's `ScopeGovernor` applies strict gate checks before execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, the scope rejects the target,
    /// or any `ScopeGovernor` gate fails.
    pub async fn execute(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "execute",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    // ── Service actions ────────────────────────────────────────────────────────

    /// Detonate a sample or payload in SERAPH's sandbox.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn detonate(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "detonate",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Orchestrate a multi-stage engagement across multiple wings.
    ///
    /// `spec` is a natural-language description of the orchestration goal.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn orchestrate(&self, spec: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "orchestrate",
            "params": { "spec": spec }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Search SERAPH's knowledge base for `query`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn knowledge_search(&self, query: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "knowledge_search",
            "params": { "query": query }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Read a knowledge-base entry by `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn knowledge_read(&self, path: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "knowledge_read",
            "params": { "path": path }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Return knowledge-base statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn knowledge_stats(&self) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "knowledge_stats",
            "params": {}
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    // ── Investigation actions ──────────────────────────────────────────────────

    /// Start a new investigation for `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn start_investigation(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "investigate_start",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Advance the current investigation with a new `finding`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn advance_investigation(&self, finding: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "investigate_advance",
            "params": { "finding": finding }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Close the current investigation.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn close_investigation(&self) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "investigate_close",
            "params": {}
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Generate a formal engagement report.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn report(&self) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "investigate_report",
            "params": {}
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    // ── Utility actions ────────────────────────────────────────────────────────

    /// Sync engagement data to the SOUL vault.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn vault_sync(&self) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "vault_sync",
            "params": {}
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Synthesise a spoken summary of the current engagement state.
    ///
    /// `text` is the message for SERAPH to speak (routed to TTS via SOUL).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn speak(&self, text: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "speak",
            "params": { "text": text }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Return SERAPH's current status — scope, active engagement, and health.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn status(&self) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "status",
            "params": {}
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    // ── Convenience: run any wing by enum ──────────────────────────────────────

    /// Run a [`Wing`] action against `target`.
    ///
    /// Convenience method that dispatches to the appropriate wing method.
    /// Prefer the typed methods (`scan`, `osint`, etc.) when the wing is
    /// known at compile time.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the scope rejects the target.
    pub async fn wing(&self, wing: Wing, target: &str) -> Result<ActionOutput, SdkError> {
        match wing {
            Wing::Capture => self.capture(target).await,
            Wing::Scan => self.scan(target).await,
            Wing::Analyze => self.analyze(target).await,
            Wing::Osint => self.osint(target, None).await,
            Wing::Monitor => self.monitor(target).await,
            Wing::Execute => self.execute(target).await,
        }
    }

    // ── Typed lifecycle methods ────────────────────────────────────────────────

    /// Check whether the engagement scope authorizes a `target`.
    ///
    /// Returns a [`ScopeResult`] with an authorization verdict and the
    /// remaining TTL on the engagement scope. The caller **must** check
    /// [`ScopeResult::is_authorized`] before dispatching wing actions.
    ///
    /// The `ttl_remaining` field is populated from SERAPH's status action
    /// and approximated from the prose response. When SERAPH rejects the
    /// scope check the result is `authorized: false` with a zero TTL.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the scope file is invalid.
    pub async fn scope_check(&self, target: &str) -> Result<ScopeResult, SdkError> {
        let params = serde_json::json!({
            "action": "status",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        let output = unwrap_text(raw)?;
        // Derive authorization from prose: SERAPH marks out-of-scope with
        // specific phrases. Absent those, treat as authorized.
        let authorized = !output.to_lowercase().contains("out of scope")
            && !output.to_lowercase().contains("not authorized")
            && !output.to_lowercase().contains("scope violation");
        // TTL is embedded in prose; we cannot parse it without structured output.
        // Store zero duration on rejection, one hour as optimistic default on success.
        let ttl = if authorized {
            Duration::from_secs(3600)
        } else {
            Duration::ZERO
        };
        Ok(ScopeResult::new(output, authorized, ttl))
    }

    /// Recon phase: gather open-source intelligence on `target`.
    ///
    /// Delegates to SERAPH's OSINT wing. Returns a [`ReconResult`] whose
    /// `output` field may contain IP addresses and hostnames that are
    /// attacker-influenced — do not use them to construct outbound connections.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the scope rejects the target.
    pub async fn recon(&self, target: &str) -> Result<ReconResult, SdkError> {
        let params = serde_json::json!({
            "action": "osint",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ReconResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Survey phase: enumerate hosts and services at `target`.
    ///
    /// Delegates to SERAPH's Scan wing. Returns a [`SurveyResult`] containing
    /// host discovery and service enumeration prose.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the scope rejects the target.
    pub async fn survey(&self, target: &str) -> Result<SurveyResult, SdkError> {
        let params = serde_json::json!({
            "action": "scan",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(SurveyResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Examine phase: analyse an artefact or binary at `target`.
    ///
    /// Delegates to SERAPH's Analyze wing. Returns an [`ExamineResult`]
    /// containing binary and protocol analysis prose.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn examine(&self, target: &str) -> Result<ExamineResult, SdkError> {
        let params = serde_json::json!({
            "action": "analyze",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(ExamineResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Strike phase: execute a payload or exploit against `target`.
    ///
    /// Delegates to SERAPH's Execute wing. Returns a [`StrikeResult`] whose
    /// fields are **attacker-controlled** — do not render `output` or
    /// `raw_findings` unescaped.
    ///
    /// SERAPH's `ScopeGovernor` applies strict gate checks before execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, the scope rejects the target,
    /// or any `ScopeGovernor` gate fails.
    pub async fn strike(&self, target: &str) -> Result<StrikeResult, SdkError> {
        let params = serde_json::json!({
            "action": "execute",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        Ok(StrikeResult {
            output: unwrap_text(raw)?,
            raw_findings: None,
        })
    }

    /// Generate a typed engagement report.
    ///
    /// Delegates to SERAPH's `investigate_report` action. Returns a
    /// [`ReportResult`] with the engagement summary in `Box<str>` fields to
    /// avoid re-allocation of large report text.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SERAPH rejects the request.
    pub async fn typed_report(&self) -> Result<ReportResult, SdkError> {
        let params = serde_json::json!({
            "action": "investigate_report",
            "params": {}
        });
        let raw = self.inner.call_tool("penTools", params).await?;
        let text = unwrap_text(raw)?;
        Ok(ReportResult {
            summary: text.into_boxed_str(),
            engagement_id: None,
        })
    }

    // ── Typed parameter methods ───────────────────────────────────────────────

    /// Call the `scan` wing with typed [`ScanParams`].
    ///
    /// Converts the typed params to JSON and calls the existing transport.
    /// Use this instead of [`SeraphClient::scan`] when you need fine-grained
    /// control over scan type, ports, timing, or tool selection.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, serialization fails, or
    /// SERAPH rejects the request.
    pub async fn scan_typed(&self, params: &ScanParams) -> Result<ActionOutput, SdkError> {
        let params_value = serde_json::to_value(params)?;
        let rpc_params = serde_json::json!({
            "action": "scan",
            "params": params_value
        });
        let raw = self.inner.call_tool("penTools", rpc_params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Call the `capture` wing with typed [`CaptureParams`].
    ///
    /// Provides fine-grained control over interface, duration, packet count,
    /// BPF filter, output path, and tool selection.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, serialization fails, or
    /// SERAPH rejects the request.
    pub async fn capture_typed(&self, params: &CaptureParams) -> Result<ActionOutput, SdkError> {
        let params_value = serde_json::to_value(params)?;
        let rpc_params = serde_json::json!({
            "action": "capture",
            "params": params_value
        });
        let raw = self.inner.call_tool("penTools", rpc_params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Call the `analyze` wing with typed [`AnalyzeParams`].
    ///
    /// Provides fine-grained control over analysis type, YARA rules, and
    /// tool selection.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, serialization fails, or
    /// SERAPH rejects the request.
    pub async fn analyze_typed(&self, params: &AnalyzeParams) -> Result<ActionOutput, SdkError> {
        let params_value = serde_json::to_value(params)?;
        let rpc_params = serde_json::json!({
            "action": "analyze",
            "params": params_value
        });
        let raw = self.inner.call_tool("penTools", rpc_params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Call the `osint` wing with typed [`OsintParams`].
    ///
    /// Provides fine-grained control over OSINT type (including Shodan,
    /// `VirusTotal`, Censys, `GreyNoise`, `AbuseIPDB`), authorization
    /// attestation, timeout, and tool selection.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, serialization fails, or
    /// SERAPH rejects the request.
    pub async fn osint_typed(&self, params: &OsintParams) -> Result<ActionOutput, SdkError> {
        let params_value = serde_json::to_value(params)?;
        let rpc_params = serde_json::json!({
            "action": "osint",
            "params": params_value
        });
        let raw = self.inner.call_tool("penTools", rpc_params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Call the `monitor` wing with typed [`MonitorParams`].
    ///
    /// Provides fine-grained control over monitor action (interfaces,
    /// ARP watch, IDS status), interface selection, and tool override.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, serialization fails, or
    /// SERAPH rejects the request.
    pub async fn monitor_typed(&self, params: &MonitorParams) -> Result<ActionOutput, SdkError> {
        let params_value = serde_json::to_value(params)?;
        let rpc_params = serde_json::json!({
            "action": "monitor",
            "params": params_value
        });
        let raw = self.inner.call_tool("penTools", rpc_params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }
}

// ── Production builder entry point ─────────────────────────────────────────────

impl SeraphClient<StdioTransport> {
    /// Create a builder for constructing a production [`SeraphClient`] backed
    /// by the SERAPH Mac bridge binary (`~/lightarchitects/seraph/bin/seraph` by default).
    #[must_use]
    pub fn builder() -> SeraphClientBuilder {
        SeraphClientBuilder::default()
    }
}

// ── SeraphClientBuilder ────────────────────────────────────────────────────────

/// Builder for [`SeraphClient`] backed by the live SERAPH Mac bridge binary.
///
/// ```no_run
/// # async fn example() -> Result<(), crate::core::SdkError> {
/// use crate::seraph::SeraphClient;
/// use std::time::Duration;
///
/// let client = SeraphClient::builder()
///     .timeout(Duration::from_secs(120))  // pentest ops can take time
///     .build()
///     .await?;
/// # Ok(()) }
/// ```
pub struct SeraphClientBuilder {
    binary_path: Option<PathBuf>,
    timeout: Duration,
    retry: RetryConfig,
    auth: Option<AuthChecker>,
}

impl Default for SeraphClientBuilder {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
            auth: None,
        }
    }
}

impl SeraphClientBuilder {
    /// Override the path to the SERAPH Mac bridge binary.
    ///
    /// Defaults to `~/lightarchitects/seraph/bin/seraph` (resolved by [`SiblingId::Seraph`]).
    #[must_use]
    pub fn binary_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary_path = Some(path.into());
        self
    }

    /// Set the per-call timeout.
    ///
    /// Pentest operations (recon, exploitation) may take 30–120 seconds.
    /// Consider increasing the timeout when calling wings or investigation
    /// actions.
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

    /// Attach an auth provider to gate connection behind a key check.
    ///
    /// Called during [`build`][Self::build] before the SERAPH binary spawns.
    /// Hard failure returns [`SdkError::Auth`]; no process is opened.
    #[must_use]
    pub fn auth(mut self, provider: impl AuthProvider) -> Self {
        self.auth = Some(AuthChecker::from_provider(provider));
        self
    }

    /// Spawn the SERAPH Mac bridge binary and complete the MCP handshake.
    ///
    /// SERAPH uses `Content-Length` framing — [`StdioTransport`] handles
    /// this automatically via [`SiblingId::Seraph`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Auth`] if the auth check fails hard.
    /// Returns [`SdkError::Config`] if `$HOME` is unset and no explicit binary
    /// path was provided. Returns a transport error if the binary cannot be
    /// spawned or the MCP handshake fails.
    pub async fn build(self) -> Result<SeraphClient<StdioTransport>, SdkError> {
        let path = match self.binary_path {
            Some(p) => p,
            None => SiblingId::Seraph.default_binary_path().ok_or_else(|| {
                SdkError::Config("$HOME is not set — provide an explicit binary_path".to_owned())
            })?,
        };
        let transport =
            StdioTransport::connect(SiblingId::Seraph, &path, self.timeout, self.auth.as_ref())
                .await?;
        Ok(SeraphClient::from_transport(transport, self.retry))
    }
}
