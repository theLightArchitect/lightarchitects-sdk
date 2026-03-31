//! Model registry — definitions and roles for each mathematical analysis model.

use serde::{Deserialize, Serialize};

/// Identifies a specific model in the oracle fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelId {
    /// Mistral Leanstral — Lean 4 formal theorem proving.
    Leanstral,
    /// `DeepSeek` V3.2 — step-by-step mathematical derivation.
    Deepseek,
    /// Qwen 3.5 — numerical reasoning and bounds checking.
    Qwen,
    /// Kimi K2 Thinking — deep reasoning with chain-of-thought.
    Kimi,
    /// Cogito 2.1 — structured reasoning with confidence ratings.
    Cogito,
}

/// The analytical role a model plays in the oracle fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    /// Generates machine-checkable Lean 4 proofs (gold standard).
    FormalProof,
    /// Provides step-by-step mathematical derivations.
    Derivation,
    /// Tests numerical bounds, searches for counterexamples.
    Numerical,
    /// Deep reasoning with thinking/chain-of-thought.
    Reasoning,
}

/// Determines which models to dispatch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleMode {
    /// Prove a property: Leanstral (proof) + `DeepSeek` (derivation) + Qwen (numerical).
    Prove,
    /// Optimize an algorithm: `DeepSeek` (derivation) + Qwen (numerical) + Kimi (reasoning).
    Optimize,
    /// Full fleet: all available models.
    Full,
    /// Custom: caller specifies models.
    Custom,
}

/// Configuration for a single model endpoint.
#[derive(Debug, Clone)]
pub(crate) struct ModelConfig {
    /// Model identifier.
    pub id: ModelId,
    /// Display name for reports.
    pub display: &'static str,
    /// Analytical role.
    pub role: ModelRole,
    /// API endpoint URL.
    pub endpoint: String,
    /// Model ID string sent in the API request.
    pub model_name: &'static str,
    /// How to obtain the API key.
    pub key_source: KeySource,
    /// Maximum response tokens.
    pub max_tokens: u32,
    /// System prompt prefix that shapes the model's analytical lens.
    pub prompt_prefix: &'static str,
}

/// How to obtain an API key for a model endpoint.
#[derive(Debug, Clone)]
pub(crate) enum KeySource {
    /// No key needed (e.g., local Ollama).
    None,
    /// Read from macOS Keychain.
    #[cfg(target_os = "macos")]
    Keychain {
        /// Keychain account name.
        account: &'static str,
        /// Keychain service name.
        service: &'static str,
    },
    /// Read from environment variable.
    #[allow(dead_code)] // Used on non-macOS platforms
    EnvVar(&'static str),
}

impl ModelId {
    /// Returns the default models for a given oracle mode.
    pub fn for_mode(mode: OracleMode) -> Vec<Self> {
        match mode {
            OracleMode::Prove => vec![Self::Leanstral, Self::Deepseek, Self::Qwen],
            OracleMode::Optimize => vec![Self::Deepseek, Self::Qwen, Self::Kimi],
            OracleMode::Full => vec![
                Self::Leanstral,
                Self::Deepseek,
                Self::Qwen,
                Self::Kimi,
                Self::Cogito,
            ],
            OracleMode::Custom => vec![], // Caller provides
        }
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Leanstral => write!(f, "leanstral"),
            Self::Deepseek => write!(f, "deepseek"),
            Self::Qwen => write!(f, "qwen"),
            Self::Kimi => write!(f, "kimi"),
            Self::Cogito => write!(f, "cogito"),
        }
    }
}

/// Build the default model configurations.
///
/// The Ollama endpoint is configurable; Mistral always uses its public API.
pub(crate) fn default_configs(ollama_endpoint: &str) -> Vec<ModelConfig> {
    vec![
        ModelConfig {
            id: ModelId::Leanstral,
            display: "Leanstral (Lean 4 Proofs)",
            role: ModelRole::FormalProof,
            endpoint: "https://api.mistral.ai/v1".to_string(),
            model_name: "labs-leanstral-2603",
            #[cfg(target_os = "macos")]
            key_source: KeySource::Keychain {
                account: "mistral",
                service: "mistral-la-plateforme-api-key",
            },
            #[cfg(not(target_os = "macos"))]
            key_source: KeySource::EnvVar("MISTRAL_API_KEY"),
            max_tokens: 8192,
            prompt_prefix: "Write Lean 4 proofs using Mathlib. Mark unproven gaps with `sorry` \
                           and explain what's missing. Be precise about preconditions.\n\n",
        },
        ModelConfig {
            id: ModelId::Deepseek,
            display: "DeepSeek V3.2 (Mathematical Derivation)",
            role: ModelRole::Derivation,
            endpoint: format!("{ollama_endpoint}/v1"),
            model_name: "deepseek-v3.2:cloud",
            key_source: KeySource::None,
            max_tokens: 8192,
            prompt_prefix: "You are a mathematician. Provide rigorous step-by-step derivations. \
                           Show all intermediate steps. State assumptions explicitly. \
                           If a claim is false, explain WHY with a counterexample.\n\n",
        },
        ModelConfig {
            id: ModelId::Qwen,
            display: "Qwen 3.5 (Numerical Reasoning)",
            role: ModelRole::Numerical,
            endpoint: format!("{ollama_endpoint}/v1"),
            model_name: "qwen3.5:cloud",
            key_source: KeySource::None,
            max_tokens: 8192,
            prompt_prefix: "You are a numerical analyst. Check bounds, verify inequalities, \
                           test edge cases with specific numbers. When given a claimed bound, \
                           try to find inputs that VIOLATE it. If it holds, prove it tightly.\n\n",
        },
        ModelConfig {
            id: ModelId::Kimi,
            display: "Kimi K2 Thinking (Deep Reasoning)",
            role: ModelRole::Reasoning,
            endpoint: format!("{ollama_endpoint}/v1"),
            model_name: "kimi-k2-thinking:cloud",
            key_source: KeySource::None,
            max_tokens: 8192,
            prompt_prefix: "Think step by step about this mathematical problem. Consider \
                           all edge cases. If the claim is true, find the tightest bound. \
                           If false, provide the simplest counterexample.\n\n",
        },
        ModelConfig {
            id: ModelId::Cogito,
            display: "Cogito 2.1 (Reasoning Specialist)",
            role: ModelRole::Reasoning,
            endpoint: format!("{ollama_endpoint}/v1"),
            model_name: "cogito-2.1:671b-cloud",
            key_source: KeySource::None,
            max_tokens: 8192,
            prompt_prefix: "You are a reasoning specialist. Analyze this mathematical claim. \
                           Structure your analysis as: (1) Restate the claim precisely, \
                           (2) Identify assumptions, (3) Prove or disprove, (4) State confidence.\n\n",
        },
    ]
}
