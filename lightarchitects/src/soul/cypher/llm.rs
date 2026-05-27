//! LLM-backed Cypher generator.
//!
//! Sends a schema description + natural-language question to an
//! OpenAI-compatible chat completions endpoint and returns a
//! sanitized, injection-hardened [`CypherQuery`].
//!
//! # Security Layers
//!
//! 1. [`sanitize`] blocks mutation keywords in the generated template.
//! 2. `$helix_id` is always injected as a Bolt **parameter** — the driver
//!    binds it at the protocol level, never parsing it as Cypher.
//! 3. Quoted string literals are stripped before the keyword scan, so
//!    `CONTAINS 'call'` does not trigger the `CALL` blocklist.
//!
//! # Endpoint
//!
//! Any OpenAI-compatible `/v1/chat/completions` server works:
//! - Ollama (default): `http://localhost:11434/v1`
//! - OpenAI Cloud: `https://api.openai.com/v1`
//! - vLLM, LM Studio, Jan, etc.

use std::collections::BTreeMap;

use serde_json::json;
use tracing::{debug, warn};

use super::sanitizer::sanitize;
use super::{CypherError, CypherQuery, GraphSchema};

// ============================================================================
// Config
// ============================================================================

/// Configuration for an LLM-backed Cypher generator.
#[derive(Debug, Clone)]
pub struct LlmCypherConfig {
    /// Base URL of the OpenAI-compatible endpoint (without trailing `/`).
    ///
    /// Defaults to `http://localhost:11434/v1` (local Ollama).
    /// For Ollama Cloud use `https://api.ollama.com/v1` (requires `api_key`).
    pub endpoint: String,
    /// Model name used for query generation.
    ///
    /// A code-specialized instruction-tuned model produces the most precise
    /// Cypher. Defaults to `devstral-small-2:24b` (Mistral Devstral Small 2
    /// on Ollama Cloud — 128K context, tool-capable, strong structured output).
    pub model: String,
    /// Optional API key sent as a `Bearer` token.
    ///
    /// Not required for local Ollama. Set for OpenAI-compatible cloud APIs.
    pub api_key: Option<String>,
    /// When set, `$helix_id` is automatically injected as a Bolt parameter
    /// in every generated query so the caller does not need to add it manually.
    pub helix_id: Option<String>,
    /// Schema description injected into the LLM system prompt.
    ///
    /// Use [`BENCH_SCHEMA`](super::schema::BENCH_SCHEMA) for `LongMemEval` and
    /// [`SOUL_HELIX_SCHEMA`](super::schema::SOUL_HELIX_SCHEMA) for the
    /// production SOUL helix.
    pub schema_hint: String,
}

impl Default for LlmCypherConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434/v1".into(),
            // Neo4j's official Gemma-3-4B text2cypher model (April 2025),
            // GGUF by mradermacher. Pull with:
            //   ollama pull hf.co/mradermacher/text-to-cypher-Gemma-3-4B-Instruct-2025.04.0-GGUF:Q4_K_M
            model: "hf.co/mradermacher/text-to-cypher-Gemma-3-4B-Instruct-2025.04.0-GGUF:Q4_K_M"
                .into(),
            api_key: None,
            helix_id: None,
            schema_hint: super::schema::BENCH_SCHEMA.into(),
        }
    }
}

// ============================================================================
// Generator
// ============================================================================

/// LLM-backed Cypher generator.
///
/// Implements [`AsyncCypherGenerator`] by sending the schema description and
/// question to a chat completions endpoint. The response is extracted,
/// sanitized against the mutation blocklist, and returned as a
/// [`CypherQuery`] with `$helix_id` injected as a Bolt parameter.
///
/// # Thread Safety
///
/// [`reqwest::Client`] is `Arc`-backed internally — safe to clone and share
/// across tasks. `LlmCypherGenerator` is `Send + Sync`.
pub struct LlmCypherGenerator {
    client: reqwest::Client,
    config: LlmCypherConfig,
}

impl std::fmt::Debug for LlmCypherGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmCypherGenerator")
            .field("endpoint", &self.config.endpoint)
            .field("model", &self.config.model)
            .finish_non_exhaustive()
    }
}

impl LlmCypherGenerator {
    /// Create a new generator with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`reqwest::Error`] if the HTTP client cannot be initialised
    /// (e.g., TLS stack failure on the platform — rare in practice).
    pub fn new(config: LlmCypherConfig) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder().build()?;
        Ok(Self { client, config })
    }

    /// Generate a Cypher query by calling the LLM endpoint.
    ///
    /// This is the async implementation called by [`AsyncCypherGenerator`].
    /// Extracted here so the trait impl stays minimal.
    ///
    /// # Errors
    ///
    /// Returns [`CypherError::Unsupported`] when the LLM request fails,
    /// no Cypher is found in the response, or the sanitizer rejects the query.
    pub async fn generate_query(&self, question: &str) -> Result<CypherQuery, CypherError> {
        let raw = self
            .call_llm(question)
            .await
            .map_err(|e| CypherError::Unsupported(e.to_string()))?;

        let cypher = extract_cypher(&raw).ok_or_else(|| {
            CypherError::Unsupported(format!(
                "no Cypher found in LLM response (first 200 chars): {}",
                &raw[..raw.len().min(200)]
            ))
        })?;

        // Normalize: text2cypher fine-tuned models output multi-line Cypher
        // with literal `\n` (backslash-n) between clauses. Neo4j rejects the
        // backslash — collapse to single-line before sanitizing.
        let cypher = normalize_cypher(&cypher);

        // Ensure the query is scoped to $helix_id when one is configured.
        // Fine-tuned models sometimes omit the filter or use a placeholder
        // literal (`'YOUR_HELIX_ID'`) instead of the Bolt parameter.
        let cypher = if self.config.helix_id.is_some() {
            force_helix_scope(&cypher)
        } else {
            cypher
        };

        sanitize(&cypher).map_err(|e| CypherError::Unsupported(format!("sanitize: {e}")))?;

        Ok(build_query(cypher, self.config.helix_id.as_deref()))
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    async fn call_llm(&self, question: &str) -> Result<String, LlmCallError> {
        let system = build_system_prompt(&self.config.schema_hint);
        let user = format!(
            "Question: {question}\n\n\
             Generate a Cypher query. Return ONLY the Cypher — no markdown, no explanation."
        );

        let url = format!(
            "{}/chat/completions",
            self.config.endpoint.trim_end_matches('/')
        );

        // POLICY EXCEPTION — non-streaming.
        //
        // The platform-wide default is `stream: true` (see contract_supervisor
        // module doc). Non-streaming is reserved for "smaller situations": this
        // Cypher generator caps output at `max_tokens: 512` (≈2 KB), produces
        // a single short structured statement, and typically completes in
        // <3 s — well inside any upstream idle window. The upstream-EOF
        // failure mode that motivated the policy applies to long-running
        // generations, not bounded short ones.
        let mut builder = self.client.post(&url).json(&json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user",   "content": user}
            ],
            "temperature": 0.0,
            "max_tokens": 512,
            "stream": false
        }));

        if let Some(key) = &self.config.api_key {
            builder = builder.bearer_auth(key);
        }

        let resp = builder.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmCallError::Status(format!("{status}: {body}")));
        }

        let body: serde_json::Value = resp.json().await?;
        body["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| LlmCallError::Parse(format!("unexpected shape: {body}")))
    }
}

// ============================================================================
// AsyncCypherGenerator impl
// ============================================================================

impl super::AsyncCypherGenerator for LlmCypherGenerator {
    async fn generate_async(
        &self,
        _schema: &GraphSchema,
        question: &str,
    ) -> Result<CypherQuery, CypherError> {
        self.generate_query(question).await
    }
}

// ============================================================================
// Error type (module-private)
// ============================================================================

#[derive(Debug)]
enum LlmCallError {
    Request(reqwest::Error),
    Status(String),
    Parse(String),
}

impl std::fmt::Display for LlmCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(e) => write!(f, "HTTP request: {e}"),
            Self::Status(s) => write!(f, "non-success status: {s}"),
            Self::Parse(s) => write!(f, "response parse: {s}"),
        }
    }
}

impl From<reqwest::Error> for LlmCallError {
    fn from(e: reqwest::Error) -> Self {
        Self::Request(e)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Collapse multi-line Cypher to a single line.
///
/// Text2cypher fine-tuned models output pretty-printed Cypher with literal
/// `\n` (backslash + n) between clauses. Neo4j rejects the backslash
/// character — replace and collapse all whitespace runs to single spaces.
fn normalize_cypher(cypher: &str) -> String {
    // `"\\n"` in Rust source = the two-character sequence backslash + 'n'.
    cypher
        .replace("\\n", " ")
        .replace(['\n', '\r'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Ensure the query is scoped to the `$helix_id` Bolt parameter.
///
/// Text2cypher fine-tuned models sometimes:
/// - Use a placeholder literal (`'YOUR_HELIX_ID'`, `'YOUR_SESSION_ID'`)
/// - Omit the `helix_id` filter entirely
///
/// This function normalises both cases so every query targets only the
/// ephemeral per-question helix rather than the entire graph.
fn force_helix_scope(cypher: &str) -> String {
    const PLACEHOLDERS: &[&str] = &[
        "'YOUR_HELIX_ID'",
        "'YOUR_SESSION_ID'",
        "'HELIX_ID'",
        "\"YOUR_HELIX_ID\"",
        "\"YOUR_SESSION_ID\"",
        "\"HELIX_ID\"",
    ];

    let mut result = cypher.to_owned();
    for ph in PLACEHOLDERS {
        result = result.replace(ph, "$helix_id");
    }

    // If $helix_id or helix_id is now referenced, we're done.
    if result.contains("helix_id") {
        return result;
    }

    // No helix_id filter at all — inject into the first `(s:Step)` pattern.
    // `replacen(..., 1)` targets only the first occurrence, leaving any
    // secondary Step nodes in joined patterns untouched.
    result.replacen("(s:Step)", "(s:Step {helix_id: $helix_id})", 1)
}

fn build_system_prompt(schema_hint: &str) -> String {
    format!(
        "You are a read-only Neo4j Cypher query generator.\n\n\
         SCHEMA:\n{schema_hint}\n\n\
         RULES:\n\
         - Output ONLY the Cypher query (no markdown fences, no explanation).\n\
         - Use MATCH + RETURN only. Never CREATE, MERGE, DELETE, SET, REMOVE, DROP, CALL.\n\
         - Always scope to the helix: MATCH (s:Step {{helix_id: $helix_id}})\n\
         - Return session IDs as: RETURN DISTINCT s.title AS session_id"
    )
}

/// Extract a Cypher query from the raw LLM response text.
///
/// Handles three common LLM output formats:
/// 1. Fenced code block: ` ```cypher\n...\n``` `
/// 2. Generic fenced block: ` ```\n...\n``` `
/// 3. Raw Cypher (first non-blank word is `MATCH`, `OPTIONAL`, or `WITH`)
fn extract_cypher(text: &str) -> Option<String> {
    let text = text.trim();

    if let Some(inner) = try_extract_fenced(text, "```cypher") {
        debug!(len = inner.len(), "extracted from cypher fence");
        return Some(inner.trim().to_owned());
    }
    if let Some(inner) = try_extract_fenced(text, "```") {
        debug!(len = inner.len(), "extracted from generic fence");
        return Some(inner.trim().to_owned());
    }

    let upper = text.to_uppercase();
    let first = upper.split_whitespace().next().unwrap_or("");
    if first == "MATCH" || first == "OPTIONAL" || first == "WITH" {
        return Some(text.to_owned());
    }

    warn!(
        preview = &text[..text.len().min(200)],
        "no Cypher pattern found in LLM output"
    );
    None
}

fn try_extract_fenced(text: &str, fence: &str) -> Option<String> {
    let start = text.find(fence)?;
    let after_open = start + fence.len();
    // Skip the optional language tag line (e.g. the "cypher" in "```cypher\n").
    let after_newline = text[after_open..].find('\n').map(|n| after_open + n + 1)?;
    let close = text[after_newline..]
        .find("```")
        .map(|n| after_newline + n)?;
    Some(text[after_newline..close].to_owned())
}

/// Build a `CypherQuery` with the `$helix_id` Bolt parameter pre-injected.
fn build_query(cypher: String, helix_id: Option<&str>) -> CypherQuery {
    let mut params = BTreeMap::new();
    if let Some(hid) = helix_id {
        params.insert("helix_id".into(), json!(hid));
    }
    CypherQuery { cypher, params }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_from_cypher_fence() {
        let text = "Here is the query:\n```cypher\nMATCH (s:Step) RETURN s\n```\n";
        assert_eq!(
            extract_cypher(text),
            Some("MATCH (s:Step) RETURN s".to_owned())
        );
    }

    #[test]
    fn extract_from_generic_fence() {
        let text = "```\nMATCH (s:Step) RETURN s\n```";
        assert_eq!(
            extract_cypher(text),
            Some("MATCH (s:Step) RETURN s".to_owned())
        );
    }

    #[test]
    fn extract_raw_match() {
        let text = "MATCH (s:Step) RETURN s.title";
        assert_eq!(extract_cypher(text), Some(text.to_owned()));
    }

    #[test]
    fn extract_raw_optional_match() {
        let text = "OPTIONAL MATCH (s:Step) RETURN s.title";
        assert_eq!(extract_cypher(text), Some(text.to_owned()));
    }

    #[test]
    fn extract_none_from_garbage() {
        assert!(extract_cypher("This is not a Cypher query.").is_none());
    }

    #[test]
    fn build_query_injects_helix_id() {
        let cq = build_query("MATCH (s) RETURN s.title".into(), Some("bench-abc"));
        assert_eq!(
            cq.params.get("helix_id").and_then(|v| v.as_str()),
            Some("bench-abc")
        );
        assert!(cq.cypher.contains("MATCH"));
    }

    #[test]
    fn build_query_no_helix_id_empty_params() {
        let cq = build_query("MATCH (s) RETURN s.title".into(), None);
        assert!(cq.params.is_empty());
    }
}
