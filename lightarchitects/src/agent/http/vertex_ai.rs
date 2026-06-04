//! Stub providers for production Google Vertex AI
//! (`{region}-aiplatform.googleapis.com`).
//!
//! Vertex hosts BOTH the native Gemini API and the Anthropic Messages API
//! ("Claude on Vertex") under separate publisher paths — these are different
//! wire shapes and require separate provider impls. Both share:
//!
//! - `OAuth2` service-account auth via GCP Application Default Credentials
//!   (NOT API key — that is Google AI Studio at
//!   `generativelanguage.googleapis.com`, served by
//!   [`super::google_ai_studio::GoogleAiStudioProvider`]).
//! - Regional URL routing (`{region}-aiplatform.googleapis.com`).
//! - GCP project id in the URL path.
//!
//! Wire-shape differences (per QUANTUM research 2026-06-04):
//!
//! | | Gemini path | Claude path |
//! |---|---|---|
//! | URL | `.../publishers/google/models/{model}:generateContent` | `.../publishers/anthropic/models/{model}:rawPredict` |
//! | Wire | Google native `generateContent` | Anthropic Messages API with `anthropic_version: "vertex-2023-10-16"` body field |
//! | Tools | `google_function_calling` | `anthropic_tool_use` |
//!
//! # Status
//!
//! Both providers are **stubs** as of 2026-06-04. Calling `spawn()` returns
//! [`ProviderError::Internal`] with a message pointing to the planned wire shape.
//! The corresponding contracts already exist:
//!
//! - `standards/canon/contracts/provider.llm/vertex-ai-gemini.yaml`
//! - `standards/canon/contracts/provider.llm/vertex-ai-claude.yaml`
//!
//! The full impl is a follow-up `/BUILD` (out of scope for the schema-shipping
//! milestone). Until then the stubs let the contracts have a corresponding Rust
//! provider id so the conformance harness can detect "provider not implemented"
//! cleanly rather than silently routing to the misnamed Google AI Studio
//! provider.

use async_trait::async_trait;

use crate::agent::{
    AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, SanitizedAgentRequest,
    SchemaMode,
};

// ── Gemini on Vertex AI ──────────────────────────────────────────────────────

/// Stub provider for Google Gemini models on production Vertex AI.
///
/// See module docs for the wire shape and the planned impl path.
pub struct VertexAiGeminiProvider {
    project_id: String,
    region: String,
    model: String,
    max_tokens: u32,
}

impl VertexAiGeminiProvider {
    /// Construct a stub provider for Gemini on Vertex AI.
    ///
    /// `project_id` is a GCP project id; `region` is a Vertex AI region (e.g.
    /// `"us-central1"`) or `"global"`; `model` is a Gemini model id (e.g.
    /// `"gemini-2.5-pro"`).
    #[must_use]
    pub fn new(
        project_id: impl Into<String>,
        region: impl Into<String>,
        model: impl Into<String>,
        max_tokens: u32,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            region: region.into(),
            model: model.into(),
            max_tokens,
        }
    }

    /// Planned base URL for this configuration.
    ///
    /// Returned for diagnostics + AYIN span enrichment even though the
    /// provider does not yet dispatch against it.
    #[must_use]
    pub fn planned_base_url(&self) -> String {
        format!(
            "https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/google/models/{model}:generateContent",
            region = self.region,
            project = self.project_id,
            model = self.model,
        )
    }
}

#[async_trait]
impl LlmAgentProvider for VertexAiGeminiProvider {
    fn name(&self) -> &'static str {
        "vertex-ai-gemini"
    }

    async fn spawn(&self, _req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        Err(ProviderError::Internal(format!(
            "VertexAiGeminiProvider is a stub (2026-06-04); planned URL: {planned} \
             (max_output_tokens={max}). See contract \
             canon://contracts/provider.llm.vertex-ai-gemini for the wire shape and \
             complete this impl in a follow-up /BUILD before promoting the \
             contract's status_per_provider cell from UNTESTED.",
            planned = self.planned_base_url(),
            max = self.max_tokens,
        )))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::None,
            native_budget_cap: false,
            native_turn_cap: false,
            auth_inherits_session: false,
        }
    }

    fn estimate_cost(&self, _input_tokens: u32, _max_output_tokens: u32) -> f64 {
        // Stub never dispatches; cost surface lands when the real impl ships
        // (Gemini 2.5 Pro list price ~$1.50/M input, $9.00/M output per
        // QUANTUM research 2026-06-04).
        0.0
    }
}

// ── Claude on Vertex AI ──────────────────────────────────────────────────────

/// Anthropic-published `anthropic_version` body field value required by the
/// Vertex AI Anthropic Messages endpoint.
///
/// Per `https://docs.anthropic.com/en/api/claude-on-vertex-ai` (accessed
/// 2026-06-04 via QUANTUM research): every request body MUST include
/// `anthropic_version: "vertex-2023-10-16"`. Omitting it yields HTTP 400.
pub const VERTEX_ANTHROPIC_VERSION: &str = "vertex-2023-10-16";

/// Stub provider for Anthropic Claude models on production Vertex AI.
///
/// See module docs for the wire shape and the planned impl path.
pub struct VertexAiClaudeProvider {
    project_id: String,
    region: String,
    model: String,
    max_tokens: u32,
}

impl VertexAiClaudeProvider {
    /// Construct a stub provider for Claude on Vertex AI.
    ///
    /// `project_id` is a GCP project id; `region` is a Vertex AI region where
    /// the Claude model is published (availability is region-restricted —
    /// e.g. `"us-east5"`); `model` is a Claude model id (e.g.
    /// `"claude-sonnet-4-6"`).
    #[must_use]
    pub fn new(
        project_id: impl Into<String>,
        region: impl Into<String>,
        model: impl Into<String>,
        max_tokens: u32,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            region: region.into(),
            model: model.into(),
            max_tokens,
        }
    }

    /// Planned base URL for this configuration.
    #[must_use]
    pub fn planned_base_url(&self) -> String {
        format!(
            "https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/anthropic/models/{model}:rawPredict",
            region = self.region,
            project = self.project_id,
            model = self.model,
        )
    }
}

#[async_trait]
impl LlmAgentProvider for VertexAiClaudeProvider {
    fn name(&self) -> &'static str {
        "vertex-ai-claude"
    }

    async fn spawn(&self, _req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        Err(ProviderError::Internal(format!(
            "VertexAiClaudeProvider is a stub (2026-06-04); planned URL: {planned} \
             (max_tokens={max}). Body MUST include anthropic_version: \"{version}\". \
             See contract canon://contracts/provider.llm.vertex-ai-claude for the \
             wire shape and complete this impl in a follow-up /BUILD before \
             promoting the contract's status_per_provider cell from UNTESTED.",
            planned = self.planned_base_url(),
            max = self.max_tokens,
            version = VERTEX_ANTHROPIC_VERSION,
        )))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::None,
            native_budget_cap: false,
            native_turn_cap: false,
            auth_inherits_session: false,
        }
    }

    fn estimate_cost(&self, _input_tokens: u32, _max_output_tokens: u32) -> f64 {
        // Stub never dispatches; cost surface lands when the real impl ships
        // (Claude Sonnet 4.6 list price ~$3/M input, $15/M output as of
        // 2026-06-04; Anthropic-on-Vertex inherits the same rate card).
        0.0
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
//
// The stub providers' `spawn()` return paths are not exercised here — the
// canonical way to build a `SanitizedAgentRequest` is via
// `AgentRequest { ... }.sanitize()`, which carries ~14 required fields and is
// boilerplate this stub does not warrant. The error message shape is verified
// by inspection of the format string above; integration coverage lands with
// the real impl. Tests below verify the construction-time contracts: name,
// capabilities, planned URLs, and constants.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_planned_base_url_uses_real_vertex_host() {
        let p = VertexAiGeminiProvider::new("my-proj", "us-central1", "gemini-2.5-pro", 4096);
        let url = p.planned_base_url();
        assert!(url.contains("us-central1-aiplatform.googleapis.com"));
        assert!(url.contains("publishers/google"));
        assert!(!url.contains("generativelanguage.googleapis.com"));
    }

    #[test]
    fn claude_planned_base_url_uses_real_vertex_host() {
        let p = VertexAiClaudeProvider::new("my-proj", "us-east5", "claude-sonnet-4-6", 4096);
        let url = p.planned_base_url();
        assert!(url.contains("us-east5-aiplatform.googleapis.com"));
        assert!(url.contains("publishers/anthropic"));
        assert!(url.contains("rawPredict"));
    }

    #[test]
    fn provider_names_match_contract_ids() {
        let g = VertexAiGeminiProvider::new("p", "us-central1", "gemini-2.5-pro", 4096);
        let c = VertexAiClaudeProvider::new("p", "us-east5", "claude-sonnet-4-6", 4096);
        assert_eq!(g.name(), "vertex-ai-gemini");
        assert_eq!(c.name(), "vertex-ai-claude");
    }

    #[test]
    fn capabilities_are_conservative_stub() {
        let g = VertexAiGeminiProvider::new("p", "us-central1", "gemini-2.5-pro", 4096);
        let caps = g.capabilities();
        assert!(matches!(caps.schema_enforcement, SchemaMode::None));
        assert!(!caps.native_budget_cap);
        assert!(!caps.native_turn_cap);
        assert!(!caps.auth_inherits_session);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn estimate_cost_is_zero_for_stub() {
        let g = VertexAiGeminiProvider::new("p", "us-central1", "gemini-2.5-pro", 4096);
        let c = VertexAiClaudeProvider::new("p", "us-east5", "claude-sonnet-4-6", 4096);
        assert_eq!(g.estimate_cost(1000, 1000), 0.0);
        assert_eq!(c.estimate_cost(1000, 1000), 0.0);
    }

    #[test]
    fn vertex_anthropic_version_matches_anthropic_docs() {
        assert_eq!(VERTEX_ANTHROPIC_VERSION, "vertex-2023-10-16");
    }
}
