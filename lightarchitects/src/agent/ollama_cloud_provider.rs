//! Ollama Cloud coding provider — executes autonomous coding tasks via structured
//! LLM output parsed and validated before writing to a worktree.
//!
//! Implements the `ironclaw-autonomous-e2e` Phase 3 Ollama coding path:
//!
//! 1. Send structured coding prompt to Ollama Cloud `/api/chat` (NDJSON stream).
//! 2. Collect the streamed response into a single UTF-8 string.
//! 3. Parse `## File: <rel_path>` code blocks and `## Commit: <message>`.
//! 4. Validate every block via [`OllamaResponseValidator`] (4 security gates).
//! 5. Write validated files to the worktree and `git add -A && git commit`.
//!
//! # AYIN Spans
//!
//! | Span event | When emitted |
//! |------------|-------------|
//! | `ollama_worker.spawn` | Instrument span on `execute_task` entry |
//! | `ollama_worker.llm_call_ms` | After HTTP response collected |
//! | `ollama_worker.response_validate` | After all blocks pass validation |
//! | `ollama_worker.commit_prepare_ms` | After `git commit` succeeds |
//! | `ollama_worker.complete` | Full task success |
//! | `ollama_worker.fail` | Task failure (any stage) |
//! | `ollama_worker.validator_reject` | On [`ValidatorRejection`] |
//! | `ollama_worker.tokens` | Input + output token counts |
//! | `ollama_worker.cost_usd` | USD cost estimate |
//!
//! # Security Controls
//!
//! | Gate | Source |
//! |------|--------|
//! | G-TRAVERSAL | `..` in path components rejected before write |
//! | G-DENY | Denylist prefix matching for CI-attack vectors |
//! | G-SYMLINK | Parent-dir canonicalization + containment check (§63.P4) |
//! | G-CARGO | Forbidden TOML section scanning for Cargo.toml patches |
//! | G-SIZE | Total diff byte ceiling (`LIGHTSQUAD_DIFF_BYTES_MAX`) |
//! | S-auth | `Authorization: Bearer` via `SecretString` — never logged or spanned |
//! | Hard timeout | Per-task wall-clock cap (`LIGHTSQUAD_OLLAMA_TIMEOUT_S`) |

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use tracing::{error, info, instrument, warn};

use crate::agent::cloud_models::lookup;
use crate::lightsquad::ollama_response_validator::{
    CodeBlock, OllamaResponseValidator, ValidatorRejection,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default Ollama coding model for ironclaw SLOT 1-3 workers.
pub const DEFAULT_CODING_MODEL: &str = "qwen3-coder:480b-cloud";

/// Default per-task hard timeout in seconds. Overridden by `LIGHTSQUAD_OLLAMA_TIMEOUT_S`.
pub const OLLAMA_TASK_TIMEOUT_DEFAULT_S: u64 = 300;

/// Ollama Cloud base URL used when `OLLAMA_HOST` is not set.
const OLLAMA_CLOUD_BASE_URL: &str = "https://ollama.com";

/// Premium tier input rate (USD per 1M tokens).
///
/// Matches `ollama.rs:PREMIUM_INPUT_USD_PER_M` for consistency in cost reporting.
const PREMIUM_INPUT_USD_PER_M: f64 = 5.00;

/// Premium tier output rate (USD per 1M tokens).
const PREMIUM_OUTPUT_USD_PER_M: f64 = 15.00;

/// System prompt instructing the LLM to produce structured code patches.
///
/// The `## File:` + fenced block + `## Commit:` format is parsed by
/// [`OllamaCloudCodingProvider::parse_response`].
const CODING_SYSTEM_PROMPT: &str = "You are an expert software engineer implementing code changes in a Rust workspace.\n\nOutput ONLY structured file patches followed by a commit message — no prose before or after.\n\nFORMAT (mandatory, in this exact order):\n## File: relative/path/to/file.rs\n```rust\n// complete file content here (entire file, not a diff)\n```\n\n## File: another/path.rs\n```rust\n// complete file content\n```\n\n## Commit: type(scope): one-line conventional commit message\n\nRULES:\n- Every ## File: block must contain the COMPLETE replacement file — not a diff or excerpt.\n- Paths are relative to the worktree root — never absolute, never containing '..'.\n- Only one ## Commit: line is allowed; place it after all ## File: blocks.\n- Use conventional commit format: feat / fix / refactor / chore / test / docs + (scope).\n- Do NOT include any explanation, preamble, or commentary outside the structured format.\n";

// ── TaskOutcome ───────────────────────────────────────────────────────────────

/// Result of a successful [`OllamaCloudCodingProvider::execute_task`] call.
#[derive(Debug)]
pub struct TaskOutcome {
    /// Absolute paths of all files written to the worktree.
    pub files_written: Vec<PathBuf>,
    /// Conventional commit message extracted from the `## Commit:` block.
    pub commit_message: String,
    /// Estimated input token count (prompt byte length / 4).
    pub input_tokens: u32,
    /// Estimated output token count (response byte length / 4).
    pub output_tokens: u32,
    /// Estimated USD cost at Premium tier rates.
    pub cost_usd: f64,
    /// Wall-clock milliseconds spent waiting for the Ollama HTTP response.
    pub llm_call_ms: u64,
}

// ── CodingProviderError ───────────────────────────────────────────────────────

/// Error returned by [`OllamaCloudCodingProvider`].
#[derive(Debug, thiserror::Error)]
pub enum CodingProviderError {
    /// Model slug not present in `CLOUD_MODEL_REGISTRY`.
    #[error("unknown Ollama model '{0}' — not in CLOUD_MODEL_REGISTRY")]
    UnknownModel(String),

    /// HTTP transport error communicating with the Ollama API.
    #[error("Ollama HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// The LLM response contained no parseable `## File:` blocks.
    #[error("LLM response contained no ## File: blocks")]
    NoFileBlocks,

    /// A security gate in [`OllamaResponseValidator`] rejected a code block.
    #[error("validator rejected block: {0}")]
    Validation(#[from] ValidatorRejection),

    /// File I/O failure while writing validated content to the worktree.
    #[error("file I/O error writing '{path}': {source}")]
    FileIo {
        /// Destination path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// `git add -A && git commit` subprocess failed.
    #[error("git commit failed: {0}")]
    GitCommit(String),

    /// Task exceeded the configured per-task timeout.
    #[error("task timed out after {0}s")]
    Timeout(u64),

    /// Prompt was rejected by G1 content sanitization.
    #[error("prompt sanitization failed: {0}")]
    SanitizationFailed(String),
}

// ── OllamaCloudCodingProvider ─────────────────────────────────────────────────

/// Executes autonomous coding tasks by querying Ollama Cloud and writing
/// validated file patches to a git worktree.
///
/// One instance is created per ironclaw SLOT (slots 1-3 in the
/// [`TierRouter`]). Slots 4-7 use [`ClaudeCliProvider`] instead.
///
/// [`TierRouter`]: crate::lightsquad::worker_spawn::TierRouter
/// [`ClaudeCliProvider`]: crate::agent::ClaudeCliProvider
#[derive(Debug)]
pub struct OllamaCloudCodingProvider {
    /// Model slug — must be present in `CLOUD_MODEL_REGISTRY`.
    pub model: String,
    /// Shared HTTP client for Ollama API calls.
    client: reqwest::Client,
    /// Ollama API base URL (from `OLLAMA_HOST` or [`OLLAMA_CLOUD_BASE_URL`]).
    base_url: String,
    /// Bearer token — `SecretString` ensures zeroed-on-drop, never logged or spanned.
    auth_token: Option<SecretString>,
    /// Per-task hard timeout.
    task_timeout: Duration,
    /// Security validator applied to every parsed code block before write.
    validator: OllamaResponseValidator,
}

impl OllamaCloudCodingProvider {
    /// Construct a provider for the given model slug.
    ///
    /// Reads `OLLAMA_HOST` for the server base URL; falls back to
    /// [`OLLAMA_CLOUD_BASE_URL`]. Reads `LIGHTSQUAD_OLLAMA_TIMEOUT_S` for the
    /// per-task timeout; falls back to [`OLLAMA_TASK_TIMEOUT_DEFAULT_S`].
    ///
    /// The `auth_token` is passed explicitly — callers should read `OLLAMA_API_KEY`
    /// once at startup to avoid the TOCTOU race documented in gate
    /// `OLLAMA_API_KEY_TOCTOU`.
    ///
    /// # Errors
    ///
    /// Returns [`CodingProviderError::UnknownModel`] if `model` is not in
    /// `CLOUD_MODEL_REGISTRY`.
    pub fn new(
        model: impl Into<String>,
        auth_token: Option<SecretString>,
    ) -> Result<Self, CodingProviderError> {
        let slug = model.into();
        if lookup(&slug).is_none() {
            return Err(CodingProviderError::UnknownModel(slug));
        }
        let base_url =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| OLLAMA_CLOUD_BASE_URL.to_owned());
        let timeout_s = std::env::var("LIGHTSQUAD_OLLAMA_TIMEOUT_S")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(OLLAMA_TASK_TIMEOUT_DEFAULT_S);
        Ok(Self {
            model: slug,
            client: reqwest::Client::new(),
            base_url,
            auth_token,
            task_timeout: Duration::from_secs(timeout_s),
            validator: OllamaResponseValidator::new(),
        })
    }

    /// Construct a provider using [`DEFAULT_CODING_MODEL`] (or
    /// `LIGHTSQUAD_CODING_MODEL` env var override) and `OLLAMA_API_KEY`
    /// from the environment.
    ///
    /// When `LIGHTSQUAD_CODING_MODEL` is set the registry check is skipped so
    /// that local Ollama models (e.g. `llama3.2:3b`) can be used without being
    /// present in `CLOUD_MODEL_REGISTRY`. This path is intended for local
    /// integration tests only — production deployments should leave the variable
    /// unset.
    ///
    /// # Errors
    ///
    /// Returns [`CodingProviderError::UnknownModel`] if no override is set and
    /// [`DEFAULT_CODING_MODEL`] is not in `CLOUD_MODEL_REGISTRY`.
    pub fn default_coding() -> Result<Self, CodingProviderError> {
        let token = std::env::var("OLLAMA_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| SecretString::new(s.into()));
        if let Ok(override_model) = std::env::var("LIGHTSQUAD_CODING_MODEL") {
            // Local / dev path — skip registry validation.
            let base_url =
                std::env::var("OLLAMA_HOST").unwrap_or_else(|_| OLLAMA_CLOUD_BASE_URL.to_owned());
            let timeout_s = std::env::var("LIGHTSQUAD_OLLAMA_TIMEOUT_S")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(OLLAMA_TASK_TIMEOUT_DEFAULT_S);
            return Ok(Self {
                model: override_model,
                client: reqwest::Client::new(),
                base_url,
                auth_token: token,
                task_timeout: Duration::from_secs(timeout_s),
                validator: OllamaResponseValidator::new(),
            });
        }
        Self::new(DEFAULT_CODING_MODEL, token)
    }

    /// Execute a coding task: send the prompt to Ollama Cloud, validate the
    /// response, write file patches to the worktree, and create a git commit.
    ///
    /// Emits AYIN D8 spans throughout the pipeline for compression benchmarking
    /// (Canon XXXVI).
    ///
    /// # Errors
    ///
    /// Returns [`CodingProviderError`] if any stage fails: HTTP transport,
    /// security gate rejection, file I/O, git subprocess, or timeout.
    #[instrument(
        name = "ollama_worker.spawn",
        skip(self, prompt, worktree_root),
        fields(task_id = %task_id, model = %self.model)
    )]
    pub async fn execute_task(
        &self,
        task_id: &str,
        prompt: &str,
        worktree_root: &Path,
    ) -> Result<TaskOutcome, CodingProviderError> {
        let input_tokens = u32::try_from(prompt.len() / 4).unwrap_or(u32::MAX);

        let body = json!({
            "model": self.model,
            "stream": true,
            "messages": [
                { "role": "system", "content": CODING_SYSTEM_PROMPT },
                { "role": "user", "content": prompt },
            ],
        });

        // ── LLM call ──────────────────────────────────────────────────────────
        let t_llm = Instant::now();
        let raw = tokio::time::timeout(self.task_timeout, self.chat_collect(&body))
            .await
            .map_err(|_| {
                let secs = self.task_timeout.as_secs();
                error!(task_id, timeout_s = secs, "ollama_worker.fail");
                CodingProviderError::Timeout(secs)
            })??;

        let llm_call_ms = u64::try_from(t_llm.elapsed().as_millis()).unwrap_or(u64::MAX);
        info!(task_id, llm_call_ms, "ollama_worker.llm_call_ms");

        // ── Parse structured response ──────────────────────────────────────────
        let (blocks, commit_message) = Self::parse_response(&raw);
        if blocks.is_empty() {
            warn!(
                task_id,
                "ollama_worker.fail: response had no ## File: blocks"
            );
            info!(task_id, reason = "NoFileBlocks", "ollama_worker.fail");
            return Err(CodingProviderError::NoFileBlocks);
        }

        let output_tokens = u32::try_from(raw.len() / 4).unwrap_or(u32::MAX);

        // ── Validate blocks ───────────────────────────────────────────────────
        self.validator.validate_total_diff_size(&blocks)?;

        let mut abs_paths: Vec<PathBuf> = Vec::with_capacity(blocks.len());
        for block in &blocks {
            match self.validator.validate_block(worktree_root, block) {
                Ok(abs) => abs_paths.push(abs),
                Err(e) => {
                    warn!(
                        task_id,
                        path = %block.rel_path.display(),
                        rejection = %e,
                        "ollama_worker.validator_reject"
                    );
                    info!(task_id, rejection = %e, "ollama_worker.validator_reject");
                    info!(task_id, reason = %e, "ollama_worker.fail");
                    return Err(CodingProviderError::Validation(e));
                }
            }
        }
        info!(
            task_id,
            files = blocks.len(),
            "ollama_worker.response_validate"
        );

        // ── Write files ───────────────────────────────────────────────────────
        for (block, abs) in blocks.iter().zip(abs_paths.iter()) {
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent).map_err(|source| CodingProviderError::FileIo {
                    path: abs.clone(),
                    source,
                })?;
            }
            std::fs::write(abs, &block.content).map_err(|source| CodingProviderError::FileIo {
                path: abs.clone(),
                source,
            })?;
        }

        // ── Git commit ─────────────────────────────────────────────────────────
        let t_commit = Instant::now();
        self.git_add_commit(worktree_root, &commit_message, task_id)
            .await?;
        let commit_prepare_ms = u64::try_from(t_commit.elapsed().as_millis()).unwrap_or(u64::MAX);
        info!(
            task_id,
            commit_prepare_ms, "ollama_worker.commit_prepare_ms"
        );

        // ── Cost estimate + final spans ────────────────────────────────────────
        let cost_usd = cost_estimate(input_tokens, output_tokens);
        info!(task_id, input_tokens, output_tokens, "ollama_worker.tokens");
        info!(task_id, cost_usd, "ollama_worker.cost_usd");
        info!(
            task_id,
            input_tokens,
            output_tokens,
            cost_usd,
            files = abs_paths.len(),
            "ollama_worker.complete"
        );

        Ok(TaskOutcome {
            files_written: abs_paths,
            commit_message,
            input_tokens,
            output_tokens,
            cost_usd,
            llm_call_ms,
        })
    }

    // ── Private helpers ─────────────────────────────────────────────────────────

    /// POST `body` to `/api/chat`, collect the NDJSON stream into one string.
    ///
    /// Each NDJSON line is `{"message":{"content":"..."},"done":bool}`.
    /// We concatenate `message.content` from every line that contains it.
    ///
    /// # Errors
    ///
    /// Returns [`CodingProviderError::Http`] for transport or HTTP-status errors.
    async fn chat_collect(&self, body: &serde_json::Value) -> Result<String, CodingProviderError> {
        let url = format!("{}/api/chat", self.base_url);
        let mut req = self.client.post(&url).json(body);
        if let Some(token) = &self.auth_token {
            req = req.bearer_auth(token.expose_secret());
        }
        let resp = req.send().await?.error_for_status()?;

        let bytes = resp.bytes().await?;
        let mut out = String::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_slice::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(content) = val
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                out.push_str(content);
            }
        }
        Ok(out)
    }

    /// Extract `## File:` code blocks and the `## Commit:` line from the LLM text.
    ///
    /// Returns `(blocks, commit_message)`. `commit_message` defaults to
    /// `"chore: automated commit"` when the `## Commit:` line is absent.
    ///
    /// The parser is a simple line-by-line state machine:
    /// - On `## File: <path>`: flush the previous block and open a new one.
    /// - On triple-backtick/tilde fence: toggle `in_fence`.
    /// - When `in_fence` is true: accumulate lines as file content.
    /// - On `## Commit: <msg>`: capture the commit message.
    fn parse_response(text: &str) -> (Vec<CodeBlock>, String) {
        let mut blocks: Vec<CodeBlock> = Vec::new();
        let mut commit_message = "chore: automated commit".to_owned();
        let mut current_path: Option<String> = None;
        let mut in_fence = false;
        let mut content_lines: Vec<&str> = Vec::new();

        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("## File:") {
                // Flush the previous block (if any) before starting the next.
                if let Some(path) = current_path.take() {
                    blocks.push(CodeBlock {
                        rel_path: PathBuf::from(path.trim()),
                        content: content_lines.join("\n"),
                    });
                    content_lines.clear();
                }
                in_fence = false;
                current_path = Some(rest.trim().to_owned());
                continue;
            }

            if let Some(rest) = line.strip_prefix("## Commit:") {
                rest.trim().clone_into(&mut commit_message);
                continue;
            }

            if current_path.is_some() {
                let fence_line = line.starts_with("```") || line.starts_with("~~~");
                if fence_line {
                    in_fence = !in_fence;
                    continue;
                }
                if in_fence {
                    content_lines.push(line);
                }
            }
        }

        // Flush the final block.
        if let Some(path) = current_path.take() {
            blocks.push(CodeBlock {
                rel_path: PathBuf::from(path.trim()),
                content: content_lines.join("\n"),
            });
        }

        (blocks, commit_message)
    }

    /// Run `git add -A && git commit -m <message>` in `worktree_root`.
    ///
    /// Appends `ironclaw-task-id: <task_id>` to the commit body for audit
    /// traceability in the HMAC decision chain.
    ///
    /// # Errors
    ///
    /// Returns [`CodingProviderError::GitCommit`] if either subprocess fails.
    async fn git_add_commit(
        &self,
        worktree_root: &Path,
        message: &str,
        task_id: &str,
    ) -> Result<(), CodingProviderError> {
        let add = tokio::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(worktree_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| CodingProviderError::GitCommit(format!("git add failed to spawn: {e}")))?;

        if !add.status.success() {
            let stderr = String::from_utf8_lossy(&add.stderr);
            return Err(CodingProviderError::GitCommit(format!(
                "git add -A failed (task {task_id}): {stderr}"
            )));
        }

        let commit_body = format!("{message}\n\nironclaw-task-id: {task_id}");
        let commit = tokio::process::Command::new("git")
            .args(["commit", "-m", &commit_body])
            .current_dir(worktree_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                CodingProviderError::GitCommit(format!("git commit failed to spawn: {e}"))
            })?;

        if !commit.status.success() {
            let stderr = String::from_utf8_lossy(&commit.stderr);
            return Err(CodingProviderError::GitCommit(format!(
                "git commit failed (task {task_id}): {stderr}"
            )));
        }

        Ok(())
    }
}

// ── Cost estimate ─────────────────────────────────────────────────────────────

/// Estimate USD cost using the Premium tier rates (matches `qwen3-coder:480b-cloud`).
///
/// Uses the same rate constants as `ollama.rs:cost_for_tier(CostTier::Premium, ...)`.
fn cost_estimate(input_tokens: u32, output_tokens: u32) -> f64 {
    (f64::from(input_tokens) / 1_000_000.0 * PREMIUM_INPUT_USD_PER_M)
        + (f64::from(output_tokens) / 1_000_000.0 * PREMIUM_OUTPUT_USD_PER_M)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── Constructor ──────────────────────────────────────────────────────────────

    #[test]
    fn new_with_unknown_model_returns_error() {
        let err = OllamaCloudCodingProvider::new("not-a-model", None).unwrap_err();
        assert!(matches!(err, CodingProviderError::UnknownModel(_)));
    }

    #[test]
    fn new_with_valid_slug_succeeds() {
        let p = OllamaCloudCodingProvider::new("glm-5.1:cloud", None).unwrap();
        assert_eq!(p.model, "glm-5.1:cloud");
    }

    #[test]
    fn default_coding_model_is_in_registry() {
        assert!(
            crate::agent::cloud_models::lookup(DEFAULT_CODING_MODEL).is_some(),
            "DEFAULT_CODING_MODEL '{DEFAULT_CODING_MODEL}' must be in CLOUD_MODEL_REGISTRY"
        );
    }

    // ── parse_response ───────────────────────────────────────────────────────────

    #[test]
    fn parse_single_block_with_commit() {
        let text = "## File: src/lib.rs\n```rust\npub fn hello() {}\n```\n\n## Commit: feat(core): add hello";
        let (blocks, commit) = OllamaCloudCodingProvider::parse_response(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].rel_path, PathBuf::from("src/lib.rs"));
        assert_eq!(blocks[0].content, "pub fn hello() {}");
        assert_eq!(commit, "feat(core): add hello");
    }

    #[test]
    fn parse_multiple_blocks() {
        let text = "## File: src/a.rs\n```rust\nmod a;\n```\n## File: src/b.rs\n```rust\nmod b;\n```\n## Commit: feat: add modules";
        let (blocks, commit) = OllamaCloudCodingProvider::parse_response(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].rel_path, PathBuf::from("src/a.rs"));
        assert_eq!(blocks[0].content, "mod a;");
        assert_eq!(blocks[1].rel_path, PathBuf::from("src/b.rs"));
        assert_eq!(blocks[1].content, "mod b;");
        assert_eq!(commit, "feat: add modules");
    }

    #[test]
    fn parse_default_commit_when_absent() {
        let text = "## File: src/lib.rs\n```rust\nfn main() {}\n```";
        let (blocks, commit) = OllamaCloudCodingProvider::parse_response(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(commit, "chore: automated commit");
    }

    #[test]
    fn parse_empty_text_returns_no_blocks() {
        let (blocks, commit) = OllamaCloudCodingProvider::parse_response("");
        assert!(blocks.is_empty());
        assert_eq!(commit, "chore: automated commit");
    }

    #[test]
    fn parse_commit_only_no_blocks() {
        let (blocks, commit) = OllamaCloudCodingProvider::parse_response("## Commit: feat: solo");
        assert!(blocks.is_empty());
        assert_eq!(commit, "feat: solo");
    }

    #[test]
    fn parse_fence_markers_excluded_from_content() {
        let text = "## File: main.rs\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n## Commit: chore: test";
        let (blocks, _) = OllamaCloudCodingProvider::parse_response(text);
        assert_eq!(blocks.len(), 1);
        assert!(
            !blocks[0].content.contains("```"),
            "fence markers must not appear in content"
        );
        assert!(blocks[0].content.contains("println!"));
    }

    #[test]
    fn parse_multiline_content_preserved() {
        let text =
            "## File: src/lib.rs\n```rust\nline1\nline2\nline3\n```\n## Commit: chore: lines";
        let (blocks, _) = OllamaCloudCodingProvider::parse_response(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "line1\nline2\nline3");
    }

    // ── cost_estimate ────────────────────────────────────────────────────────────

    #[test]
    fn cost_zero_tokens_is_zero() {
        assert!(cost_estimate(0, 0) < f64::EPSILON);
    }

    #[test]
    fn cost_positive_for_nonzero_tokens() {
        assert!(cost_estimate(1_000, 1_000) > 0.0);
    }

    #[test]
    fn cost_scales_linearly() {
        let c1 = cost_estimate(1_000_000, 0);
        let c2 = cost_estimate(2_000_000, 0);
        assert!(
            (c2 - 2.0 * c1).abs() < 1e-9,
            "cost must scale linearly with token count"
        );
    }
}
