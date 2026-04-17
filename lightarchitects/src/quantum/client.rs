//! [`QuantumClient`] — typed client for the QUANTUM MCP server.

use std::path::PathBuf;
use std::time::Duration;

use crate::core::auth::AuthChecker;
use crate::core::constants::DEFAULT_TIMEOUT_SECS;
use crate::core::error::SdkError;
use crate::core::transport::Transport;
use crate::core::{AuthProvider, McpClient, RetryConfig, SiblingId, StdioTransport};

use crate::quantum::content::unwrap_text;
use crate::quantum::types::{
    CloseResult, DiscoverResult, HelixResult, ListResult, ProbeResult, QuickResult, ResearchResult,
    SweepResult, TheorizeResult, TraceResult, TriageResult, VerifyResult, WorkflowResult,
};

// ── QuantumClient ──────────────────────────────────────────────────────────────

/// Typed client for the QUANTUM MCP server (`qsTools` — 13 actions).
///
/// QUANTUM is a forensic investigation server. All 13 actions drive an
/// evidence-chain investigation cycle:
///
/// ```text
/// SCAN → SWEEP → TRACE → PROBE → THEORIZE → VERIFY → CLOSE
///   └── utility actions: quick, research, helix, discover, list, workflow
/// ```
///
/// Every action returns AI-generated prose — hypothesis chains, evidence
/// summaries, or investigation status. There are no structured-JSON responses.
///
/// Constructed via [`QuantumClient::builder`] (production, spawns the QUANTUM
/// binary with the required `mcp-server` subcommand) or
/// [`QuantumClient::from_transport`] (testing, injects a [`Transport`]).
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), crate::core::SdkError> {
/// use crate::quantum::QuantumClient;
///
/// let client = QuantumClient::builder().build().await?;
///
/// // Begin an investigation
/// let evidence = client.scan("unexpected auth failures in prod").await?;
/// println!("{}", evidence.output);
///
/// // Form a hypothesis
/// let theory = client.theorize("expired session tokens", None).await?;
/// println!("{}", theory.output);
///
/// // Verify against the helix vault
/// let helix = client.helix("auth failures", None).await?;
/// println!("{}", helix.output);
/// # Ok(()) }
/// ```
pub struct QuantumClient<T: Transport> {
    inner: McpClient<T>,
}

impl<T: Transport> QuantumClient<T> {
    /// Construct a client from an already-connected transport.
    ///
    /// Intended for testing — pass a `MockTransport` to exercise all methods
    /// without spawning a real QUANTUM binary.
    pub fn from_transport(transport: T, retry: RetryConfig) -> Self {
        Self {
            inner: McpClient::new(transport, retry),
        }
    }

    /// Wrap an existing [`McpClient`].
    ///
    /// Use this to reuse a connection already managed by an `McpManager`.
    /// The `McpClient` is `Clone`, so the original connection remains valid.
    pub fn from_client(inner: McpClient<T>) -> Self {
        Self { inner }
    }

    // ── Investigation cycle actions ─────────────────────────────────────────────

    /// Begin an initial evidence scan for `subject`.
    ///
    /// `scan` is the entry point of the QUANTUM investigation cycle. It
    /// collects first-pass evidence from logs, helix data, and available
    /// signals for the given subject.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    /// Alias for [`QuantumClient::triage`] — preserved for backward compatibility.
    ///
    /// Prefer `triage()` in new code. Both methods call the `triage` action
    /// (the server-side `scan` alias is accepted but deprecated).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    #[deprecated(since = "0.2.0", note = "use `triage()` instead")]
    pub async fn scan(&self, subject: &str) -> Result<TriageResult, SdkError> {
        self.triage(subject).await
    }

    /// Phase 1 — initial evidence discovery (`triage`).
    ///
    /// Entry point of the QUANTUM investigation cycle. Collects first-pass
    /// evidence from logs, helix data, and available signals for `subject`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn triage(&self, subject: &str) -> Result<TriageResult, SdkError> {
        let params = serde_json::json!({
            "action": "triage",
            "params": { "subject": subject }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(TriageResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Perform a broader sweep of evidence for `subject`.
    ///
    /// `sweep` expands the investigation scope beyond the initial scan,
    /// collecting cross-signal evidence from all available sources.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn sweep(&self, subject: &str) -> Result<SweepResult, SdkError> {
        let params = serde_json::json!({
            "action": "sweep",
            "params": { "subject": subject }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(SweepResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Trace an evidence chain for `subject`.
    ///
    /// `trace` follows a specific evidence thread, constructing a causal chain
    /// from raw signals to observable behaviour.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn trace(&self, subject: &str) -> Result<TraceResult, SdkError> {
        let params = serde_json::json!({
            "action": "trace",
            "params": { "subject": subject }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(TraceResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Deep-probe a specific `target` within an investigation.
    ///
    /// `probe` performs focused, high-depth analysis of a single target —
    /// a file, symbol, process, or hypothesis — to extract maximum signal.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn probe(&self, target: &str) -> Result<ProbeResult, SdkError> {
        let params = serde_json::json!({
            "action": "probe",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(ProbeResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Form hypotheses from the accumulated evidence for `subject`.
    ///
    /// `theorize` synthesises collected evidence into ranked hypotheses with
    /// confidence scores and supporting rationale. Optional `context` provides
    /// additional framing (e.g. prior investigation notes).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn theorize(
        &self,
        subject: &str,
        context: Option<&str>,
    ) -> Result<TheorizeResult, SdkError> {
        let mut p = serde_json::json!({ "subject": subject });
        if let Some(ctx) = context {
            p["context"] = serde_json::Value::String(ctx.to_owned());
        }
        let params = serde_json::json!({ "action": "theorize", "params": p });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(TheorizeResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Verify a `hypothesis` against available evidence.
    ///
    /// `verify` tests a specific hypothesis by cross-referencing it against
    /// all collected signals. Returns a verdict with supporting or refuting
    /// evidence and a confidence assessment.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn verify(&self, hypothesis: &str) -> Result<VerifyResult, SdkError> {
        let params = serde_json::json!({
            "action": "verify",
            "params": { "hypothesis": hypothesis }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(VerifyResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Close the current investigation and produce a final report.
    ///
    /// `close` terminates the active investigation, summarises all evidence
    /// chains and verified hypotheses, and writes a final helix entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn close(&self, summary: &str) -> Result<CloseResult, SdkError> {
        let params = serde_json::json!({
            "action": "close",
            "params": { "summary": summary }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(CloseResult {
            output: unwrap_text(raw)?,
        })
    }

    // ── Utility actions ─────────────────────────────────────────────────────────

    /// Run a quick investigation of `subject` (abbreviated cycle).
    ///
    /// `quick` compresses the full investigation cycle into a single fast pass:
    /// scan → theorize → verify → close. Suitable for low-complexity questions
    /// that don't warrant a full multi-step investigation.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn quick(&self, subject: &str) -> Result<QuickResult, SdkError> {
        let params = serde_json::json!({
            "action": "quick",
            "params": { "subject": subject }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(QuickResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Research a `topic` across multiple knowledge sources.
    ///
    /// `research` queries documentation, helix memory, web sources, and
    /// `HuggingFace` papers for the given topic. Returns a synthesised summary
    /// with citations.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn research(&self, topic: &str) -> Result<ResearchResult, SdkError> {
        let params = serde_json::json!({
            "action": "research",
            "params": { "topic": topic }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(ResearchResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Query the SOUL helix vault for entries related to `query`.
    ///
    /// `helix` provides direct access to the consciousness knowledge graph —
    /// filtering by sibling, strands, significance, or free-text search.
    /// Optional `sibling` restricts results to a specific helix spine
    /// (e.g. `"eva"`, `"corso"`, `"quantum"`).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn helix(&self, query: &str, sibling: Option<&str>) -> Result<HelixResult, SdkError> {
        let mut p = serde_json::json!({ "query": query });
        if let Some(s) = sibling {
            p["sibling"] = serde_json::Value::String(s.to_owned());
        }
        let params = serde_json::json!({ "action": "helix", "params": p });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(HelixResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Discover patterns in `target` (codebase path, log stream, or dataset).
    ///
    /// `discover` performs unsupervised pattern recognition over the target,
    /// surfacing anomalies, correlations, and recurring structures.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn discover(&self, target: &str) -> Result<DiscoverResult, SdkError> {
        let params = serde_json::json!({
            "action": "discover",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(DiscoverResult {
            output: unwrap_text(raw)?,
        })
    }

    /// List active or past investigations.
    ///
    /// Returns a summary of the investigation history — active cases, closed
    /// cases, and their current status.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn list(&self) -> Result<ListResult, SdkError> {
        let params = serde_json::json!({
            "action": "list",
            "params": {}
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(ListResult {
            output: unwrap_text(raw)?,
        })
    }

    /// Execute a named workflow `name`.
    ///
    /// Workflows are pre-defined investigation sequences stored in QUANTUM's
    /// configuration. They encode repeatable investigation patterns (e.g.
    /// `"auth-audit"`, `"dep-scan"`) that combine multiple actions.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or QUANTUM rejects the request.
    pub async fn workflow(&self, name: &str) -> Result<WorkflowResult, SdkError> {
        let params = serde_json::json!({
            "action": "workflow",
            "params": { "name": name }
        });
        let raw = self.inner.call_tool("qsTools", params).await?;
        Ok(WorkflowResult {
            output: unwrap_text(raw)?,
        })
    }
}

// ── Production builder entry point ─────────────────────────────────────────────

impl QuantumClient<StdioTransport> {
    /// Create a builder for constructing a production [`QuantumClient`] backed
    /// by the QUANTUM binary (`~/lightarchitects/quantum/bin/quantum-q` by default).
    ///
    /// The builder automatically passes the required `mcp-server` subcommand
    /// to the binary. QUANTUM is the only sibling that requires a subcommand.
    #[must_use]
    pub fn builder() -> QuantumClientBuilder {
        QuantumClientBuilder::default()
    }
}

// ── QuantumClientBuilder ───────────────────────────────────────────────────────

/// Builder for [`QuantumClient`] backed by a live QUANTUM binary.
///
/// ```no_run
/// # async fn example() -> Result<(), crate::core::SdkError> {
/// use crate::quantum::QuantumClient;
/// use std::time::Duration;
///
/// let client = QuantumClient::builder()
///     .timeout(Duration::from_secs(120))  // investigations can take time
///     .build()
///     .await?;
/// # Ok(()) }
/// ```
pub struct QuantumClientBuilder {
    binary_path: Option<PathBuf>,
    timeout: Duration,
    retry: RetryConfig,
    auth: Option<AuthChecker>,
}

impl Default for QuantumClientBuilder {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
            auth: None,
        }
    }
}

impl QuantumClientBuilder {
    /// Override the path to the QUANTUM binary.
    ///
    /// Defaults to `~/lightarchitects/quantum/bin/quantum-q` (resolved by [`SiblingId::Quantum`]).
    #[must_use]
    pub fn binary_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary_path = Some(path.into());
        self
    }

    /// Set the per-call timeout.
    ///
    /// QUANTUM investigations — particularly `theorize`, `verify`, and
    /// multi-source `research` — may take 30–120 seconds. Consider increasing
    /// the timeout when calling these actions.
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
    /// Called during [`build`][Self::build] before the QUANTUM binary spawns.
    /// Hard failure returns [`SdkError::Auth`]; no process is opened.
    #[must_use]
    pub fn auth(mut self, provider: impl AuthProvider) -> Self {
        self.auth = Some(AuthChecker::from_provider(provider));
        self
    }

    /// Spawn the QUANTUM binary and complete the MCP handshake.
    ///
    /// Automatically passes the `mcp-server` subcommand required by QUANTUM
    /// (see [`SiblingId::mcp_subcommand`]).
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Auth`] if the auth check fails hard.
    /// Returns [`SdkError::Config`] if `$HOME` is unset and no explicit binary
    /// path was provided. Returns a transport error if the binary cannot be
    /// spawned or the MCP handshake fails.
    pub async fn build(self) -> Result<QuantumClient<StdioTransport>, SdkError> {
        let path = match self.binary_path {
            Some(p) => p,
            None => SiblingId::Quantum.default_binary_path().ok_or_else(|| {
                SdkError::Config("$HOME is not set — provide an explicit binary_path".to_owned())
            })?,
        };
        let transport =
            StdioTransport::connect(SiblingId::Quantum, &path, self.timeout, self.auth.as_ref())
                .await?;
        Ok(QuantumClient::from_transport(transport, self.retry))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::core::{McpClient, MockTransport, RetryConfig};

    use crate::quantum::QuantumClient;

    /// `from_client` routes `research()` through the mock transport.
    #[tokio::test]
    async fn from_client_research_parses_prose_response() {
        let payload = serde_json::json!({
            "content": [{ "type": "text", "text": "Investigation complete: Rust ownership is safe." }],
            "isError": false
        });
        let transport = MockTransport::ok(payload);
        let inner = McpClient::new(transport, RetryConfig::default());
        let quantum = QuantumClient::from_client(inner);

        let result = quantum.research("Rust ownership").await.expect("mock ok");
        assert!(
            result.output.contains("Investigation complete"),
            "got: {}",
            result.output
        );
    }
}
