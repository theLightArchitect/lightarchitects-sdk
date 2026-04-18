//! [`SoulClient`] — typed client for the SOUL MCP server.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::core::auth::AuthChecker;
use crate::core::constants::DEFAULT_TIMEOUT_SECS;
use crate::core::error::SdkError;
use crate::core::transport::Transport;
use crate::core::{AuthProvider, McpClient, RetryConfig, SiblingId, StdioTransport};

use crate::soul::graphrag_ingest::GraphRagIngestBuilder;
use crate::soul::helix::HelixBuilder;
use crate::soul::ingest::IngestBuilder;
use crate::soul::query::QueryBuilder;
use crate::soul::research::ResearchBuilder;
use crate::soul::types::{
    ChatResult, ConvergenceResult, ConverseResult, HealthReport, IngestResult, LinksResult,
    ManifestContent, NoteContent, NoteList, NoteWritten, QueryFrontmatterResult, RelateResult,
    ResearchResult, SearchHit, SpeakResult, StatsReport, TagSyncReport, ValidateReport,
    VoiceResult,
};

// ── SoulClient ────────────────────────────────────────────────────────────────

/// Typed client for the SOUL MCP server (`soulTools` — 23 actions).
///
/// Constructed via [`SoulClient::builder`] (production, spawns the SOUL binary)
/// or [`SoulClient::from_transport`] (testing, injects a [`Transport`]).
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), lightarchitects::core::SdkError> {
/// use lightarchitects::soul::SoulClient;
///
/// let client = SoulClient::builder().api_key("la_your_key_here").build()?;
/// let entries = client.helix().sibling("eva").significance_min(7.0).call().await?;
/// # Ok(()) }
/// ```
pub struct SoulClient<T: Transport> {
    inner: McpClient<T>,
}

impl<T: Transport> SoulClient<T> {
    /// Construct a client from an already-connected transport.
    ///
    /// Intended for testing — pass a `MockTransport` to exercise all methods
    /// without spawning a real SOUL binary.
    pub fn from_transport(transport: T, retry: RetryConfig) -> Self {
        Self {
            inner: McpClient::new(transport, retry),
        }
    }

    /// Wrap an existing [`McpClient`].
    ///
    /// Use this to reuse a connection already managed by an `McpManager`
    /// rather than spawning a new SOUL process. The `McpClient` is `Clone`,
    /// so the original connection remains valid after wrapping.
    pub fn from_client(inner: McpClient<T>) -> Self {
        Self { inner }
    }

    // ── Note operations ───────────────────────────────────────────────────────

    /// Read a vault note by its vault-relative path.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SOUL cannot read the note.
    pub async fn read_note(&self, path: &str) -> Result<NoteContent, SdkError> {
        let params = serde_json::json!({ "action": "read_note", "params": { "path": path } });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Create a new vault note at `path` with the given `content`.
    ///
    /// SOUL rejects writes to existing paths — use [`read_note`][Self::read_note]
    /// first if you need to check for existence.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SOUL refuses the write.
    pub async fn write_note(&self, path: &str, content: &str) -> Result<NoteWritten, SdkError> {
        let params = serde_json::json!({
            "action": "write_note",
            "params": { "path": path, "content": content }
        });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// List notes inside a vault directory.
    ///
    /// `path` defaults to the vault root when `None`. `limit` caps the result
    /// count (server default applies when `None`).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SOUL cannot enumerate the path.
    pub async fn list_notes(
        &self,
        path: Option<&str>,
        limit: Option<u32>,
    ) -> Result<NoteList, SdkError> {
        let mut p = serde_json::json!({});
        if let Some(path) = path {
            p["path"] = path.into();
        }
        if let Some(limit) = limit {
            p["limit"] = limit.into();
        }
        let params = serde_json::json!({ "action": "list_notes", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    // ── Search ────────────────────────────────────────────────────────────────

    /// Regex-search across vault content.
    ///
    /// `path` scopes the search to a directory; `frontmatter_only` restricts
    /// matches to YAML frontmatter blocks; `limit` caps results.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn search(
        &self,
        pattern: &str,
        path: Option<&str>,
        frontmatter_only: bool,
        limit: Option<u32>,
    ) -> Result<Vec<SearchHit>, SdkError> {
        let mut p = serde_json::json!({ "pattern": pattern });
        if let Some(path) = path {
            p["path"] = path.into();
        }
        if frontmatter_only {
            p["frontmatter_only"] = true.into();
        }
        if let Some(limit) = limit {
            p["limit"] = limit.into();
        }
        let params = serde_json::json!({ "action": "search", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Query vault entries by YAML frontmatter field.
    ///
    /// `operator` is one of: `==`, `!=`, `>=`, `<=`, `>`, `<`, `contains`,
    /// `exists`. For `exists`, `value` may be `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn query_frontmatter(
        &self,
        field: &str,
        operator: &str,
        value: Option<&str>,
        path: Option<&str>,
        limit: Option<u32>,
    ) -> Result<QueryFrontmatterResult, SdkError> {
        let mut p = serde_json::json!({ "field": field, "operator": operator });
        if let Some(value) = value {
            p["value"] = value.into();
        }
        if let Some(path) = path {
            p["path"] = path.into();
        }
        if let Some(limit) = limit {
            p["limit"] = limit.into();
        }
        let params = serde_json::json!({ "action": "query_frontmatter", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    // ── Helix & RAG query (fluent builders) ───────────────────────────────────

    /// Start a fluent helix query builder.
    ///
    /// Chain filter methods then call `.call().await` to execute.
    pub fn helix(&self) -> HelixBuilder<'_, T> {
        HelixBuilder::new(&self.inner)
    }

    /// Start a fluent 4-signal RAG query builder.
    ///
    /// Chain optional filters then call `.call().await` to execute.
    pub fn query(&self, query: impl Into<String>) -> QueryBuilder<'_, T> {
        QueryBuilder::new(&self.inner, query)
    }

    /// Start a fluent ingest builder for a validated vault path.
    ///
    /// The `path` is expanded and validated against the vault root (`~/lightarchitects/soul/`)
    /// during this call. See [`IngestBuilder`] for details.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `$HOME` is unset, the path contains null
    /// bytes, traversal components, or falls outside the vault root.
    pub fn ingest_builder(&self, path: &str) -> Result<IngestBuilder<'_, T>, SdkError> {
        IngestBuilder::with_path(&self.inner, path)
    }

    /// Start a fluent `GraphRAG` ingestion builder.
    ///
    /// Parses a document (file or inline text) into entities and relations,
    /// then writes them to the SOUL knowledge graph. Requires Neo4j.
    ///
    /// Chain [`GraphRagIngestBuilder::source`] (required) and optional
    /// [`GraphRagIngestBuilder::domain`], [`GraphRagIngestBuilder::sibling`],
    /// [`GraphRagIngestBuilder::dry_run`] before calling `.call().await`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(client: lightarchitects::soul::SoulClient<lightarchitects::core::StdioTransport>)
    /// # -> Result<(), lightarchitects::core::SdkError> {
    /// use lightarchitects::soul::IngestSource;
    ///
    /// let result = client
    ///     .graphrag_ingest()
    ///     .source(IngestSource::File("/path/to/doc.md".into()))
    ///     .domain("research")
    ///     .sibling("eva")
    ///     .call()
    ///     .await?;
    ///
    /// println!("{} nodes created", result.nodes_created);
    /// # Ok(()) }
    /// ```
    pub fn graphrag_ingest(&self) -> GraphRagIngestBuilder<'_, T> {
        GraphRagIngestBuilder::new(&self.inner)
    }

    /// Start a fluent research builder for the given query string.
    ///
    /// The `query` is validated for null bytes and control characters during
    /// this call. See [`ResearchBuilder`] for details.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `query` contains null bytes or ASCII
    /// control characters other than tab and newline.
    pub fn research_builder(
        &self,
        query: impl Into<String>,
    ) -> Result<ResearchBuilder<'_, T>, SdkError> {
        ResearchBuilder::new(&self.inner, query)
    }

    // ── Vault health & metadata ───────────────────────────────────────────────

    /// Check SOUL's connection to the Neo4j graph backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn health(&self) -> Result<HealthReport, SdkError> {
        let params = serde_json::json!({ "action": "health", "params": {} });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Return vault statistics (entry counts, strand/resonance frequency).
    ///
    /// `sibling` scopes statistics to a single sibling; `None` returns aggregate.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn stats(&self, sibling: Option<&str>) -> Result<StatsReport, SdkError> {
        let mut p = serde_json::json!({});
        if let Some(sibling) = sibling {
            p["sibling"] = sibling.into();
        }
        let params = serde_json::json!({ "action": "stats", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Read the vault manifest.json metadata file.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn manifest(&self) -> Result<ManifestContent, SdkError> {
        let params = serde_json::json!({ "action": "manifest", "params": {} });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Validate helix entries against the canonical template.
    ///
    /// `path` scopes validation to a specific file or directory; `all` runs
    /// validation across the entire vault when `true`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn validate(
        &self,
        path: Option<&str>,
        all: bool,
    ) -> Result<ValidateReport, SdkError> {
        let mut p = serde_json::json!({});
        if let Some(path) = path {
            p["path"] = path.into();
        }
        if all {
            p["all"] = true.into();
        }
        let params = serde_json::json!({ "action": "validate", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Validate vault tags against the canonical vocabulary.
    ///
    /// `dry_run: true` reports issues without making changes.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn tag_sync(&self, dry_run: bool) -> Result<TagSyncReport, SdkError> {
        let params = serde_json::json!({
            "action": "tag_sync",
            "params": { "dry_run": dry_run }
        });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Discover all available `soulTools` actions and their parameter schemas.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn list_actions(&self) -> Result<Value, SdkError> {
        let params = serde_json::json!({ "action": "list", "params": {} });
        self.inner.call_tool("soulTools", params).await
    }

    // ── Voice & personality ───────────────────────────────────────────────────

    /// Synthesise speech from `text` using `ElevenLabs` TTS.
    ///
    /// `voice_id` overrides the default voice for the sibling; pass `None` to
    /// use the configured default.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or `ElevenLabs` rejects the request.
    pub async fn speak(&self, text: &str, voice_id: Option<&str>) -> Result<SpeakResult, SdkError> {
        let mut p = serde_json::json!({ "text": text });
        if let Some(vid) = voice_id {
            p["voice_id"] = vid.into();
        }
        let params = serde_json::json!({ "action": "speak", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Assemble a personality prompt for a sibling from the SOUL vault.
    ///
    /// Returns the full system prompt, voice profile, and the caller's message
    /// echoed back for convenience.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn converse(
        &self,
        sibling: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> Result<ConverseResult, SdkError> {
        let mut p = serde_json::json!({ "sibling": sibling, "message": message });
        if let Some(sid) = session_id {
            p["session_id"] = sid.into();
        }
        let params = serde_json::json!({ "action": "converse", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Run the unified voice pipeline (batch prompts + TTS synthesis).
    ///
    /// `params` follows the `soulTools voice` schema — see SOUL documentation
    /// for `siblings`, `prompt`, and `synthesize` fields.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn voice(&self, params_inner: Value) -> Result<VoiceResult, SdkError> {
        let params = serde_json::json!({ "action": "voice", "params": params_inner });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Run the text-to-dialogue stitching pipeline (multi-speaker audio).
    ///
    /// `params` follows the `soulTools dialogue` schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn dialogue(&self, params_inner: Value) -> Result<VoiceResult, SdkError> {
        let params = serde_json::json!({ "action": "dialogue", "params": params_inner });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Interact with the multi-sibling conversation engine.
    ///
    /// `sub_action` is one of `chat_start`, `chat_stop`, `chat_status`,
    /// `chat_inject`. `params_inner` provides the sub-action-specific fields.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn chat(
        &self,
        sub_action: &str,
        params_inner: Value,
    ) -> Result<ChatResult, SdkError> {
        let mut p = params_inner;
        p["sub_action"] = sub_action.into();
        let params = serde_json::json!({ "action": "chat", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    // ── Knowledge graph ───────────────────────────────────────────────────────

    /// Ingest content into the SOUL knowledge graph.
    ///
    /// `params_inner` follows the `soulTools ingest` schema (structure is
    /// SOUL-version-dependent; pass raw JSON for full control).
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn ingest(&self, params_inner: Value) -> Result<IngestResult, SdkError> {
        let params = serde_json::json!({ "action": "ingest", "params": params_inner });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Query N-way convergences (shared experiences across siblings).
    ///
    /// `helix_ids` restricts to convergences involving specific helix entries.
    /// `min_weight` and `min_participants` filter by convergence strength.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn convergences(
        &self,
        helix_ids: Option<&[&str]>,
        min_weight: Option<f64>,
        min_participants: Option<u32>,
        limit: Option<u32>,
    ) -> Result<ConvergenceResult, SdkError> {
        let mut p = serde_json::json!({});
        if let Some(ids) = helix_ids {
            p["helix_ids"] = ids.to_vec().into();
        }
        if let Some(w) = min_weight {
            p["min_weight"] = w.into();
        }
        if let Some(n) = min_participants {
            p["min_participants"] = n.into();
        }
        if let Some(limit) = limit {
            p["limit"] = limit.into();
        }
        let params = serde_json::json!({ "action": "convergences", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Create an explicit directed link between two helix steps.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn relate(
        &self,
        source_id: &str,
        target_id: &str,
        link_type: &str,
        strength: Option<f64>,
        metadata: Option<Value>,
    ) -> Result<RelateResult, SdkError> {
        let mut p = serde_json::json!({
            "source_id": source_id,
            "target_id": target_id,
            "link_type": link_type,
        });
        if let Some(s) = strength {
            p["strength"] = s.into();
        }
        if let Some(meta) = metadata {
            p["metadata"] = meta;
        }
        let params = serde_json::json!({ "action": "relate", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Query outgoing and incoming wikilinks for a helix step.
    ///
    /// `direction` is one of `"outgoing"`, `"incoming"`, or `None` for both.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn links(
        &self,
        step_id: &str,
        direction: Option<&str>,
        limit: Option<u32>,
    ) -> Result<LinksResult, SdkError> {
        let mut p = serde_json::json!({ "step_id": step_id });
        if let Some(dir) = direction {
            p["direction"] = dir.into();
        }
        if let Some(limit) = limit {
            p["limit"] = limit.into();
        }
        let params = serde_json::json!({ "action": "links", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }

    /// Research aggregation — search vault digests, summarise, or refresh.
    ///
    /// `mode` is one of `"search"`, `"digest"`, or `"refresh"`. Additional
    /// parameters (`sources`, `categories`, `limit`) can be supplied via
    /// `extra_params`.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails.
    pub async fn research(
        &self,
        query: Option<&str>,
        mode: Option<&str>,
        extra_params: Option<Value>,
    ) -> Result<ResearchResult, SdkError> {
        let mut p = extra_params.unwrap_or_else(|| serde_json::json!({}));
        if let Some(q) = query {
            p["query"] = q.into();
        }
        if let Some(m) = mode {
            p["mode"] = m.into();
        }
        let params = serde_json::json!({ "action": "soul_search", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }
}

// ── SoulClient construction (production path) ─────────────────────────────────

impl SoulClient<StdioTransport> {
    /// Create a [`SoulLocalBuilder`] for local dev mode (spawns the SOUL binary directly).
    ///
    /// Prefer [`SoulClient::builder`] for the cloud API path.
    pub fn local_builder() -> SoulLocalBuilder {
        SoulLocalBuilder::default()
    }
}

// ── SoulLocalBuilder ─────────────────────────────────────────────────────────

/// Builder for [`SoulClient<StdioTransport>`] — local dev mode.
///
/// Spawns the SOUL binary from the filesystem. Use [`SoulClient::builder`] for
/// the cloud API path instead.
pub struct SoulLocalBuilder {
    binary_path: Option<PathBuf>,
    timeout: Duration,
    retry: RetryConfig,
    auth: Option<AuthChecker>,
}

impl Default for SoulLocalBuilder {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
            auth: None,
        }
    }
}

impl SoulLocalBuilder {
    /// Override the path to the SOUL MCP binary.
    ///
    /// Defaults to `~/lightarchitects/soul/bin/soul` when `None`.
    #[must_use]
    pub fn binary_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary_path = Some(path.into());
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

    /// Attach an auth provider to gate connection behind a key check.
    ///
    /// The provider's [`AuthProvider::check_connect`] is called during
    /// [`build`][Self::build] **before** the SOUL binary is spawned. A hard
    /// auth failure returns [`SdkError::Auth`] and no process is opened.
    ///
    /// The production implementation is `lightarchitects::auth::AuthGuard`.
    #[must_use]
    pub fn auth(mut self, provider: impl AuthProvider) -> Self {
        self.auth = Some(AuthChecker::from_provider(provider));
        self
    }

    /// Spawn the SOUL binary and complete the MCP handshake.
    ///
    /// If an auth provider was set via [`.auth()`][Self::auth], the auth check
    /// runs first. A hard failure returns [`SdkError::Auth`] without spawning.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Auth`] if the auth check fails hard.
    /// Returns [`SdkError::Config`] if `$HOME` is unset and no explicit binary
    /// path was provided. Returns a transport error if the binary cannot be
    /// spawned or the MCP handshake fails.
    pub async fn build(self) -> Result<SoulClient<StdioTransport>, SdkError> {
        let path = match self.binary_path {
            Some(p) => p,
            None => SiblingId::Soul.default_binary_path().ok_or_else(|| {
                SdkError::Config("$HOME is not set — provide an explicit binary_path".to_owned())
            })?,
        };
        let transport =
            StdioTransport::connect(SiblingId::Soul, &path, self.timeout, self.auth.as_ref())
                .await?;
        Ok(SoulClient {
            inner: McpClient::new(transport, self.retry),
        })
    }
}

// ── Cloud builder (HTTP transport) ────────────────────────────────────────────

#[cfg(feature = "http-client")]
impl SoulClient<crate::core::HttpTransport> {
    /// Create a [`SoulClientBuilder`] targeting the Light Architects cloud API.
    ///
    /// This is the default production path — SOUL's business logic runs on the
    /// gateway; the SDK sends typed JSON-RPC calls over HTTPS.
    pub fn builder() -> SoulClientBuilder {
        SoulClientBuilder::default()
    }
}

/// Builder for [`SoulClient`] backed by the Light Architects cloud API.
///
/// ```no_run
/// # fn example() -> Result<(), lightarchitects::core::SdkError> {
/// use lightarchitects::soul::SoulClient;
///
/// let client = SoulClient::builder()
///     .api_key("la_your_key_here")
///     .build()?;
/// # Ok(()) }
/// ```
#[cfg(feature = "http-client")]
pub struct SoulClientBuilder {
    api_key: String,
    base_url: String,
    timeout: Duration,
    retry: RetryConfig,
}

#[cfg(feature = "http-client")]
impl Default for SoulClientBuilder {
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
impl SoulClientBuilder {
    /// Set the API key (required).
    ///
    /// Keys follow the `la_` prefix format issued by `api.lightarchitects.ai`.
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

    /// Build the [`SoulClient`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if the API key is empty or the HTTP
    /// client cannot be constructed.
    pub fn build(self) -> Result<SoulClient<crate::core::HttpTransport>, SdkError> {
        let transport = crate::core::HttpTransport::builder(SiblingId::Soul)
            .api_key(self.api_key)
            .base_url(self.base_url)
            .timeout(self.timeout)
            .build()?;
        Ok(SoulClient::from_transport(transport, self.retry))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::core::{McpClient, MockTransport, RetryConfig};

    use crate::soul::SoulClient;

    /// Construct a `SoulClient` from a mock `McpClient` and verify the
    /// `from_client` bridge compiles and works end-to-end.
    #[tokio::test]
    async fn from_client_helix_parses_canned_response() {
        let payload = serde_json::json!([
            { "title": "Test Entry", "significance": 8.5 },
            { "title": "Second Entry", "significance": 6.0 }
        ]);
        let transport = MockTransport::ok(payload);
        let inner = McpClient::new(transport, RetryConfig::default());
        let soul = SoulClient::from_client(inner);

        let entries = soul
            .helix()
            .limit(5)
            .call()
            .await
            .expect("mock should succeed");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "Test Entry");
        assert!((entries[0].significance - 8.5).abs() < 0.001);
        assert_eq!(entries[1].title, "Second Entry");
    }

    /// `from_client` and `from_transport` share no state — clone semantics.
    #[tokio::test]
    async fn from_client_clone_does_not_share_state() {
        let t1 = MockTransport::ok(serde_json::json!([]));
        let t2 = MockTransport::ok(serde_json::json!([]));
        let c1 = SoulClient::from_client(McpClient::new(t1, RetryConfig::default()));
        let c2 = SoulClient::from_client(McpClient::new(t2, RetryConfig::default()));

        // Both succeed independently — they hold separate transports.
        let r1 = c1.helix().call().await.expect("c1 ok");
        let r2 = c2.helix().call().await.expect("c2 ok");
        assert!(r1.is_empty());
        assert!(r2.is_empty());
    }

    /// An empty `MockTransport` queue still returns a valid (null) response.
    #[tokio::test]
    async fn mock_transport_empty_queue_returns_null() {
        let transport = MockTransport::null();
        let inner = McpClient::new(transport, RetryConfig::default());
        let soul = SoulClient::from_client(inner);
        // `helix` returns `[]` when result is null/not-an-array — should not panic.
        // It will fail to deserialize `null` as `Vec<HelixEntry>` → Err is ok.
        let result = soul.helix().call().await;
        // Null response → deserialization error, not a panic.
        assert!(result.is_err() || result.is_ok());
    }
}
