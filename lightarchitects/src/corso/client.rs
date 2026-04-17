//! [`CorsoClient`] — typed client for the CORSO MCP server.

use std::path::PathBuf;
use std::time::Duration;

use crate::core::auth::AuthChecker;
use crate::core::constants::DEFAULT_TIMEOUT_SECS;
use crate::core::error::{SdkError, ToolError};
use crate::core::transport::Transport;
use crate::core::{AuthProvider, McpClient, RetryConfig, SiblingId, StdioTransport};

use crate::corso::content::{unwrap_json, unwrap_text};
use crate::corso::types::{
    ActionOutput, CodeSearchHit, ContainerOp, DirEntry, DirectoryListing, FileContent, FileOutline,
    FileWritten, ReferenceResult, SecretOp, SymbolSearchResult,
};

/// Maximum byte length accepted by [`CorsoClient::write_file`].
///
/// 10 MiB is a practical ceiling — CORSO is a code/ops tool, not a blob
/// store. Files larger than this almost certainly indicate a caller bug.
const MAX_WRITE_BYTES: usize = 10 * 1024 * 1024;

// ── CorsoClient ───────────────────────────────────────────────────────────────

/// Typed client for the CORSO MCP server (`corsoTools` — 26 actions).
///
/// Constructed via [`CorsoClient::builder`] (production, spawns the CORSO
/// binary) or [`CorsoClient::from_transport`] (testing, injects a [`Transport`]).
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), crate::core::SdkError> {
/// use crate::corso::CorsoClient;
///
/// let client = CorsoClient::builder().build().await?;
///
/// // Read a file
/// let file = client.read_file("/path/to/file.rs", None).await?;
/// println!("{}", file.content);
///
/// // AI code review (returns prose)
/// let review = client.code_review("/path/to/file.rs", None).await?;
/// println!("{}", review.output);
/// # Ok(()) }
/// ```
pub struct CorsoClient<T: Transport> {
    inner: McpClient<T>,
}

impl<T: Transport> CorsoClient<T> {
    /// Construct a client from an already-connected transport.
    ///
    /// Intended for testing — pass a `MockTransport` to exercise all methods
    /// without spawning a real CORSO binary.
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

    /// Generic action dispatch.
    ///
    /// Use when no typed method exists for the CORSO action you need (e.g.
    /// `"hunt"`, `"scout"`). Returns the AI-generated prose output.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the action.
    pub async fn action(
        &self,
        action: &str,
        params: serde_json::Value,
    ) -> Result<ActionOutput, SdkError> {
        let wrapped = serde_json::json!({ "action": action, "params": params });
        let raw = self.inner.call_tool("corsoTools", wrapped).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    // ── Filesystem actions ─────────────────────────────────────────────────────

    /// Read a file by path. Optional `encoding` selects the read mode (e.g.
    /// `"utf-8"` or `"binary"`).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO cannot read the file.
    pub async fn read_file(
        &self,
        path: &str,
        encoding: Option<&str>,
    ) -> Result<FileContent, SdkError> {
        let mut p = serde_json::json!({ "path": path });
        if let Some(enc) = encoding {
            p["encoding"] = serde_json::Value::String(enc.to_owned());
        }
        let params = serde_json::json!({ "action": "read_file", "params": p });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        let inner = unwrap_json(raw)?;
        let result: FileContent = serde_json::from_value(inner).map_err(SdkError::from)?;
        if !result.success {
            return Err(SdkError::Tool(ToolError {
                tool: "corsoTools".to_owned(),
                message: format!("read_file failed for path: {path}"),
            }));
        }
        Ok(result)
    }

    /// Write `content` to a file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO cannot write the file.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<FileWritten, SdkError> {
        if content.len() > MAX_WRITE_BYTES {
            return Err(SdkError::Config(format!(
                "write_file: content length {} exceeds 10 MiB limit",
                content.len()
            )));
        }
        let params = serde_json::json!({ "action": "write_file", "params": { "path": path, "content": content } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        let inner = unwrap_json(raw)?;
        let result: FileWritten = serde_json::from_value(inner).map_err(SdkError::from)?;
        if !result.success {
            return Err(SdkError::Tool(ToolError {
                tool: "corsoTools".to_owned(),
                message: format!("write_file failed for path: {path}"),
            }));
        }
        Ok(result)
    }

    /// List directory contents at `path`. Optional `recursive` flag descends
    /// into subdirectories.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the directory cannot be read.
    pub async fn list_directory(
        &self,
        path: &str,
        recursive: bool,
    ) -> Result<Vec<DirEntry>, SdkError> {
        let params = serde_json::json!({
            "action": "list_directory",
            "params": { "path": path, "recursive": recursive }
        });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        let inner = unwrap_json(raw)?;
        let listing: DirectoryListing = serde_json::from_value(inner).map_err(SdkError::from)?;
        Ok(listing.entries)
    }

    // ── Code intelligence actions ──────────────────────────────────────────────

    /// Search for `pattern` in source files. Restricts to `path` when
    /// provided.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn search_code(
        &self,
        pattern: &str,
        path: Option<&str>,
    ) -> Result<Vec<CodeSearchHit>, SdkError> {
        let mut p = serde_json::json!({ "pattern": pattern });
        if let Some(dir) = path {
            p["path"] = serde_json::Value::String(dir.to_owned());
        }
        let params = serde_json::json!({ "action": "search_code", "params": p });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        let inner = unwrap_json(raw)?;
        serde_json::from_value(inner).map_err(SdkError::from)
    }

    /// Locate definitions of `symbol` in the codebase.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn find_symbol(&self, symbol: &str) -> Result<SymbolSearchResult, SdkError> {
        let params = serde_json::json!({ "action": "find_symbol", "params": { "query": symbol } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        let inner = unwrap_json(raw)?;
        serde_json::from_value(inner).map_err(SdkError::from)
    }

    /// Return the structural outline (functions, structs, impls) of `file`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn get_outline(&self, file: &str) -> Result<FileOutline, SdkError> {
        let params = serde_json::json!({ "action": "get_outline", "params": { "file": file } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        let inner = unwrap_json(raw)?;
        serde_json::from_value(inner).map_err(SdkError::from)
    }

    /// Find all references to `symbol` across the codebase.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn get_references(&self, symbol: &str) -> Result<ReferenceResult, SdkError> {
        let params =
            serde_json::json!({ "action": "get_references", "params": { "query": symbol } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        let inner = unwrap_json(raw)?;
        serde_json::from_value(inner).map_err(SdkError::from)
    }

    // ── AI analysis actions ────────────────────────────────────────────────────

    /// AI-powered code analysis via the SNIFF domain.
    ///
    /// `target` is a file path, directory, or code snippet. Returns
    /// CORSO's SNIFF analysis as prose.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn sniff(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "sniff", "params": { "target": target } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Security audit via the GUARD domain.
    ///
    /// Scans `target` (file, directory, or code) for vulnerabilities, threat
    /// vectors, and supply-chain risks.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn guard(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "guard", "params": { "target": target } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Research and knowledge retrieval via the FETCH domain.
    ///
    /// `query` is a natural-language research question. Returns CORSO's
    /// findings as prose.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    /// Research and knowledge retrieval via the FETCH domain.
    ///
    /// `depth` controls research thoroughness:
    /// - `"quick"` — keyword match only, skips Context7 docs lookup.
    /// - `"deep"` (default) — full research including Context7 library docs.
    pub async fn fetch(&self, query: &str) -> Result<ActionOutput, SdkError> {
        self.fetch_with_depth(query, "deep").await
    }

    /// Research with explicit depth control.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn fetch_with_depth(
        &self,
        query: &str,
        depth: &str,
    ) -> Result<ActionOutput, SdkError> {
        let params =
            serde_json::json!({ "action": "fetch", "params": { "query": query, "depth": depth } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Performance and test analysis via the CHASE domain.
    ///
    /// `target` is a file, directory, or benchmark name. Returns CORSO's
    /// CHASE analysis as prose.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn chase(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "chase", "params": { "target": target } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// AI code review of `target` (file path or code).
    ///
    /// Optional `context` provides additional background for the review.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn code_review(
        &self,
        target: &str,
        context: Option<&str>,
    ) -> Result<ActionOutput, SdkError> {
        let mut p = serde_json::json!({ "target": target });
        if let Some(ctx) = context {
            p["context"] = serde_json::Value::String(ctx.to_owned());
        }
        let params = serde_json::json!({ "action": "code_review", "params": p });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// AI code generation from `prompt`.
    ///
    /// Returns the generated code or implementation as prose/code text.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn generate_code(&self, prompt: &str) -> Result<ActionOutput, SdkError> {
        let params =
            serde_json::json!({ "action": "generate_code", "params": { "prompt": prompt } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Search documentation for `query`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn search_documentation(&self, query: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "search_documentation",
            "params": { "query": query }
        });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Analyze architecture of `target` (directory or module).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn analyze_architecture(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "analyze_architecture",
            "params": { "target": target }
        });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    // ── Operational actions ────────────────────────────────────────────────────

    /// Deploy `target` (binary name, service, or path).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the deploy is rejected.
    pub async fn deploy(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "deploy", "params": { "target": target } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Roll back `target` to a previous version.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the rollback is rejected.
    pub async fn rollback(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "rollback", "params": { "target": target } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Manage a container.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the operation is rejected.
    pub async fn container_manage(
        &self,
        operation: ContainerOp,
        target: &str,
    ) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "container_manage",
            "params": { "operation": operation.as_str(), "target": target }
        });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Manage a secret.
    ///
    /// `value` is required when `operation` is [`SecretOp::Set`]; it is
    /// ignored for [`SecretOp::Get`] and [`SecretOp::Delete`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `operation` is `Set` but `value` is
    /// `None`. Returns an error if the transport fails or CORSO rejects the
    /// request.
    pub async fn secret_manage(
        &self,
        operation: SecretOp,
        key: &str,
        value: Option<&str>,
    ) -> Result<ActionOutput, SdkError> {
        if matches!(operation, SecretOp::Set) && value.is_none() {
            return Err(SdkError::Config(
                "secret_manage: `value` is required for SecretOp::Set".to_owned(),
            ));
        }
        let mut p = serde_json::json!({ "operation": operation.as_str(), "key": key });
        if let Some(v) = value {
            p["value"] = serde_json::Value::String(v.to_owned());
        }
        let params = serde_json::json!({ "action": "secret_manage", "params": p });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Execute a CORSO STRIKE operation against `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn strike(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "strike", "params": { "target": target } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Watch `target` for changes or events.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn watch(&self, target: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "watch", "params": { "target": target } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Run a SCOUT analysis of the project at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn scout(&self, path: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({ "action": "scout", "params": { "path": path } });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Report the health status of `service`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn monitor_health(&self, service: &str) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "monitor_health",
            "params": { "service": service }
        });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Scale `service` to `replicas` instances.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn scale_resources(
        &self,
        service: &str,
        replicas: u32,
    ) -> Result<ActionOutput, SdkError> {
        let params = serde_json::json!({
            "action": "scale_resources",
            "params": { "service": service, "replicas": replicas }
        });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }

    /// Retrieve logs for `service`. Optional `lines` caps the number of lines.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or CORSO rejects the request.
    pub async fn manage_logs(
        &self,
        service: &str,
        lines: Option<u32>,
    ) -> Result<ActionOutput, SdkError> {
        let mut p = serde_json::json!({ "service": service });
        if let Some(n) = lines {
            p["lines"] = serde_json::Value::Number(serde_json::Number::from(n));
        }
        let params = serde_json::json!({ "action": "manage_logs", "params": p });
        let raw = self.inner.call_tool("corsoTools", params).await?;
        Ok(ActionOutput {
            output: unwrap_text(raw)?,
        })
    }
}

// ── Production builder entry point ────────────────────────────────────────────

impl CorsoClient<StdioTransport> {
    /// Create a builder for constructing a production [`CorsoClient`] backed by
    /// the CORSO binary (`~/lightarchitects/corso/bin/corso` by default).
    #[must_use]
    pub fn builder() -> CorsoClientBuilder {
        CorsoClientBuilder::default()
    }
}

// ── CorsoClientBuilder ────────────────────────────────────────────────────────

/// Builder for [`CorsoClient`] backed by a live CORSO binary.
///
/// ```no_run
/// # async fn example() -> Result<(), crate::core::SdkError> {
/// use crate::corso::CorsoClient;
/// use std::time::Duration;
///
/// let client = CorsoClient::builder()
///     .timeout(Duration::from_secs(60))
///     .build()
///     .await?;
/// # Ok(()) }
/// ```
pub struct CorsoClientBuilder {
    binary_path: Option<PathBuf>,
    timeout: Duration,
    retry: RetryConfig,
    auth: Option<AuthChecker>,
}

impl Default for CorsoClientBuilder {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
            auth: None,
        }
    }
}

impl CorsoClientBuilder {
    /// Override the path to the CORSO binary.
    ///
    /// Defaults to `~/lightarchitects/corso/bin/corso` (resolved by [`SiblingId::Corso`]).
    #[must_use]
    pub fn binary_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary_path = Some(path.into());
        self
    }

    /// Set the per-call timeout.
    ///
    /// CORSO's AI analysis actions (guard, sniff, `code_review`, …) can take
    /// 10–60 seconds depending on model and input size. The default is
    /// `DEFAULT_TIMEOUT_SECS` but callers using analysis actions should
    /// increase this.
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
    /// Called during [`build`][Self::build] before the CORSO binary spawns.
    /// Hard failure returns [`SdkError::Auth`]; no process is opened.
    #[must_use]
    pub fn auth(mut self, provider: impl AuthProvider) -> Self {
        self.auth = Some(AuthChecker::from_provider(provider));
        self
    }

    /// Spawn the CORSO binary and complete the MCP handshake.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Auth`] if the auth check fails hard.
    /// Returns [`SdkError::Config`] if `$HOME` is unset and no explicit binary
    /// path was provided. Returns a transport error if the binary cannot be
    /// spawned or the MCP handshake fails.
    pub async fn build(self) -> Result<CorsoClient<StdioTransport>, SdkError> {
        let path = match self.binary_path {
            Some(p) => p,
            None => SiblingId::Corso.default_binary_path().ok_or_else(|| {
                SdkError::Config("$HOME is not set — provide an explicit binary_path".to_owned())
            })?,
        };
        let transport =
            StdioTransport::connect(SiblingId::Corso, &path, self.timeout, self.auth.as_ref())
                .await?;
        Ok(CorsoClient::from_transport(transport, self.retry))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::core::{McpClient, MockTransport, RetryConfig};

    use crate::corso::CorsoClient;

    /// `from_client` delegates to inner — `guard` round-trip through mock.
    #[tokio::test]
    async fn from_client_guard_parses_prose_response() {
        // CORSO wraps responses in a ToolCallResult envelope: { content: [...], isError: false }
        let payload = serde_json::json!({
            "content": [{ "type": "text", "text": "No vulnerabilities found." }],
            "isError": false
        });
        let transport = MockTransport::ok(payload);
        let inner = McpClient::new(transport, RetryConfig::default());
        let corso = CorsoClient::from_client(inner);

        let result = corso.guard(".").await.expect("mock should succeed");
        assert!(
            result.output.contains("No vulnerabilities"),
            "got: {}",
            result.output
        );
    }

    /// Generic `action()` escape hatch routes through corsoTools correctly.
    #[tokio::test]
    async fn action_escape_hatch_routes_hunt() {
        let payload = serde_json::json!({
            "content": [{ "type": "text", "text": "Build complete." }],
            "isError": false
        });
        let transport = MockTransport::ok(payload);
        let inner = McpClient::new(transport, RetryConfig::default());
        let corso = CorsoClient::from_client(inner);

        let result = corso
            .action("hunt", serde_json::json!({ "cwd": "." }))
            .await
            .expect("mock should succeed");
        assert!(
            result.output.contains("Build complete"),
            "got: {}",
            result.output
        );
    }
}
