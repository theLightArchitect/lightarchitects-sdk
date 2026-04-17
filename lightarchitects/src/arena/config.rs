//! Arena configuration parsing and validation.
//!
//! The [`ArenaConfig`](crate::arena::config::ArenaConfig) struct is the user's entry point — it deserializes from
//! a JSON file and drives the entire arena pipeline. Configuration includes
//! model endpoint, MCP server definitions, exercise parameters, and output
//! format selection.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::arena::scoring::RewardConfig;

/// Top-level arena configuration, deserialized from user JSON.
///
/// # Example
///
/// ```json
/// {
///   "model": {
///     "endpoint": "https://my-model.example.com/v1",
///     "api_key_env": "MY_MODEL_API_KEY",
///     "name": "my-fine-tuned-llama"
///   },
///   "mcp_servers": [
///     {
///       "name": "my-database",
///       "command": "npx @my-org/db-mcp-server",
///       "transport": "stdio"
///     }
///   ],
///   "exercises": {
///     "types": ["tool-selection", "parameter-filling", "multi-step-chain"],
///     "count": 500,
///     "difficulty": ["easy", "medium", "hard"]
///   },
///   "output": {
///     "formats": ["sft", "dpo", "rl"],
///     "path": "./training-data/"
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArenaConfig {
    /// LLM model endpoint configuration.
    pub model: ModelConfig,
    /// MCP servers to connect to.
    pub mcp_servers: Vec<ServerConfig>,
    /// Exercise generation parameters.
    pub exercises: ExerciseConfig,
    /// Output configuration.
    pub output: OutputConfig,
    /// Optional reward weight overrides.
    #[serde(default)]
    pub rewards: Option<RewardConfig>,
    /// Optional LLM-as-Judge configuration.
    #[serde(default)]
    pub judge: Option<JudgeConfig>,
    /// pass^k reliability configuration.
    #[serde(default)]
    pub pass_k: Option<PassKConfig>,
}

/// LLM model endpoint configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// OpenAI-compatible API endpoint URL.
    pub endpoint: String,
    /// Environment variable name containing the API key (never stored directly).
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Human-readable model name for reporting.
    pub name: String,
    /// Maximum tokens for model responses.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Temperature for model sampling.
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

/// MCP server connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique name for this server (used in exercise generation and routing).
    pub name: String,
    /// Command to spawn the server (for stdio transport).
    #[serde(default)]
    pub command: Option<String>,
    /// URL to connect to (for HTTP transport).
    #[serde(default)]
    pub url: Option<String>,
    /// Transport type.
    pub transport: TransportType,
    /// Optional environment variables to set when spawning.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Timeout for tool calls in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

/// Transport type for MCP server connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// Stdio transport (spawn process, communicate via stdin/stdout).
    Stdio,
    /// HTTP transport (connect to URL, JSON-RPC over HTTP).
    Http,
}

/// Exercise generation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseConfig {
    /// Types of exercises to generate.
    pub types: Vec<ExerciseType>,
    /// Total number of exercises to generate.
    pub count: u32,
    /// Difficulty levels to include.
    pub difficulty: Vec<Difficulty>,
    /// Random seed for deterministic generation.
    #[serde(default)]
    pub seed: Option<u64>,
}

/// Types of training exercises the arena can generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExerciseType {
    /// Pick the correct tool from N candidates.
    ToolSelection,
    /// Fill in the correct parameters for a given tool.
    ParameterFilling,
    /// Complete a task requiring tools A → B → C in sequence.
    MultiStepChain,
    /// Handle a tool failure and find an alternative approach.
    ErrorRecovery,
    /// N tools available, only some relevant — pick the right ones.
    Distractor,
    /// The answer is in the context — no tool call needed.
    NoToolNeeded,
    /// Use tools from multiple servers together.
    CrossServer,
}

/// Exercise difficulty levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    /// Single tool, obvious choice, simple parameters.
    Easy,
    /// Multiple tools, some ambiguity, nested parameters.
    Medium,
    /// Multi-step, distractors, complex schemas, edge cases.
    Hard,
}

/// Output configuration for exported training data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Training data formats to export.
    pub formats: Vec<OutputFormat>,
    /// Output directory path.
    pub path: PathBuf,
    /// Include model's reasoning chain in exports.
    #[serde(default = "default_true")]
    pub include_reasoning: bool,
    /// Include 8-dimensional reward breakdown in exports.
    #[serde(default = "default_true")]
    pub include_scores: bool,
    /// Minimum reward score for SFT examples (0.0–1.0).
    #[serde(default = "default_sft_threshold")]
    pub sft_threshold: f64,
}

/// Training data output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Supervised fine-tuning format (ChatML/ShareGPT conversations).
    Sft,
    /// Direct preference optimization format (chosen/rejected pairs).
    Dpo,
    /// Reinforcement learning format (full traces with 8-dim rewards).
    Rl,
}

/// Optional LLM-as-Judge configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeConfig {
    /// Whether LLM-as-Judge scoring is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Judge model endpoint (can differ from training model).
    pub endpoint: String,
    /// Environment variable for the judge API key.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Weight of LLM judge score relative to rule-based score (0.0–1.0).
    #[serde(default = "default_judge_weight")]
    pub weight: f64,
}

/// pass^k reliability testing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassKConfig {
    /// Number of times to run each exercise.
    pub k: u32,
    /// Report consistency metrics.
    #[serde(default = "default_true")]
    pub report_consistency: bool,
}

// ── Default value functions ─────────────────────────────────────────────────

fn default_max_tokens() -> u32 {
    2048
}

fn default_temperature() -> f32 {
    0.7
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

fn default_sft_threshold() -> f64 {
    0.7
}

fn default_judge_weight() -> f64 {
    0.3
}

// ── Config loading and validation ───────────────────────────────────────────

/// Errors that can occur during configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the configuration file.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    /// Failed to parse JSON configuration.
    #[error("failed to parse JSON config: {0}")]
    Json(#[from] serde_json::Error),
    /// Configuration validation failed.
    #[error("config validation failed: {0}")]
    Validation(String),
}

impl ArenaConfig {
    /// Load and validate configuration from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if the file cannot be read, parsed, or fails
    /// validation.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration for logical consistency.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.mcp_servers.is_empty() {
            return Err(ConfigError::Validation(
                "at least one MCP server must be configured".into(),
            ));
        }

        // Check for duplicate server names.
        let mut seen = HashSet::new();
        for server in &self.mcp_servers {
            if !seen.insert(&server.name) {
                return Err(ConfigError::Validation(format!(
                    "duplicate server name: {}",
                    server.name
                )));
            }

            // Each server must have either a command (stdio) or URL (http).
            match server.transport {
                TransportType::Stdio => {
                    if server.command.is_none() {
                        return Err(ConfigError::Validation(format!(
                            "server '{}' uses stdio transport but has no command",
                            server.name
                        )));
                    }
                }
                TransportType::Http => {
                    if server.url.is_none() {
                        return Err(ConfigError::Validation(format!(
                            "server '{}' uses http transport but has no url",
                            server.name
                        )));
                    }
                }
            }
        }

        if self.exercises.count == 0 {
            return Err(ConfigError::Validation(
                "exercise count must be greater than 0".into(),
            ));
        }

        if self.exercises.types.is_empty() {
            return Err(ConfigError::Validation(
                "at least one exercise type must be specified".into(),
            ));
        }

        if self.exercises.difficulty.is_empty() {
            return Err(ConfigError::Validation(
                "at least one difficulty level must be specified".into(),
            ));
        }

        // Cross-server exercises require 2+ servers.
        if self.exercises.types.contains(&ExerciseType::CrossServer) && self.mcp_servers.len() < 2 {
            return Err(ConfigError::Validation(
                "cross-server exercises require at least 2 MCP servers".into(),
            ));
        }

        // SFT threshold must be 0.0–1.0.
        if !(0.0..=1.0).contains(&self.output.sft_threshold) {
            return Err(ConfigError::Validation(format!(
                "sft_threshold must be 0.0–1.0, got {}",
                self.output.sft_threshold
            )));
        }

        // Judge weight must be 0.0–1.0.
        if let Some(judge) = &self.judge {
            if !(0.0..=1.0).contains(&judge.weight) {
                return Err(ConfigError::Validation(format!(
                    "judge weight must be 0.0–1.0, got {}",
                    judge.weight
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_config_json() -> &'static str {
        r#"{
  "model": { "endpoint": "http://localhost:8080/v1", "name": "test-model" },
  "mcp_servers": [{ "name": "test-server", "command": "echo hello", "transport": "stdio" }],
  "exercises": { "types": ["tool-selection"], "count": 10, "difficulty": ["easy"] },
  "output": { "formats": ["sft"], "path": "./out/" }
}"#
    }

    #[test]
    fn parses_minimal_config() {
        let config: ArenaConfig =
            serde_json::from_str(minimal_config_json()).expect("should parse");
        assert_eq!(config.model.name, "test-model");
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.exercises.count, 10);
        assert_eq!(config.output.formats.len(), 1);
    }

    #[test]
    fn validates_empty_servers() {
        let json = r#"{
  "model": { "endpoint": "http://localhost:8080/v1", "name": "test" },
  "mcp_servers": [],
  "exercises": { "types": ["tool-selection"], "count": 10, "difficulty": ["easy"] },
  "output": { "formats": ["sft"], "path": "./out/" }
}"#;
        let config: ArenaConfig = serde_json::from_str(json).expect("should parse");
        let err = config.validate().expect_err("should fail");
        assert!(err.to_string().contains("at least one MCP server"));
    }

    #[test]
    fn validates_stdio_requires_command() {
        let json = r#"{
  "model": { "endpoint": "http://localhost:8080/v1", "name": "test" },
  "mcp_servers": [{ "name": "bad", "transport": "stdio" }],
  "exercises": { "types": ["tool-selection"], "count": 10, "difficulty": ["easy"] },
  "output": { "formats": ["sft"], "path": "./out/" }
}"#;
        let config: ArenaConfig = serde_json::from_str(json).expect("should parse");
        let err = config.validate().expect_err("should fail");
        assert!(err.to_string().contains("no command"));
    }

    #[test]
    fn validates_cross_server_needs_two_servers() {
        let json = r#"{
  "model": { "endpoint": "http://localhost:8080/v1", "name": "test" },
  "mcp_servers": [{ "name": "only-one", "command": "echo", "transport": "stdio" }],
  "exercises": { "types": ["cross-server"], "count": 10, "difficulty": ["easy"] },
  "output": { "formats": ["sft"], "path": "./out/" }
}"#;
        let config: ArenaConfig = serde_json::from_str(json).expect("should parse");
        let err = config.validate().expect_err("should fail");
        assert!(err.to_string().contains("at least 2 MCP servers"));
    }

    #[test]
    fn default_values_applied() {
        let config: ArenaConfig =
            serde_json::from_str(minimal_config_json()).expect("should parse");
        assert_eq!(config.model.max_tokens, 2048);
        assert!((config.model.temperature - 0.7).abs() < f32::EPSILON);
        assert!(config.output.include_reasoning);
        assert!(config.output.include_scores);
        assert!((config.output.sft_threshold - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn exercise_types_deserialize_kebab_case() {
        let json = r#"{
  "model": { "endpoint": "http://localhost/v1", "name": "test" },
  "mcp_servers": [
    { "name": "a", "command": "echo", "transport": "stdio" },
    { "name": "b", "command": "echo", "transport": "stdio" }
  ],
  "exercises": {
    "types": ["tool-selection", "parameter-filling", "multi-step-chain", "error-recovery", "distractor", "no-tool-needed", "cross-server"],
    "count": 100,
    "difficulty": ["easy", "medium", "hard"]
  },
  "output": { "formats": ["sft", "dpo", "rl"], "path": "./out/" }
}"#;
        let config: ArenaConfig = serde_json::from_str(json).expect("should parse");
        assert_eq!(config.exercises.types.len(), 7);
        assert_eq!(config.output.formats.len(), 3);
        config.validate().expect("should validate");
    }
}
