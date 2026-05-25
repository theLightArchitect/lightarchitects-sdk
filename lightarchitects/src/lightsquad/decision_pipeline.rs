//! 4-layer decision pipeline — Layer 0 (Categorical) → Canon → Northstar → LightArchitect → User.
//!
//! ADR-002 introduces [`CategoricalExclusion`] as a Layer 0 pre-screen that fires
//! unconditionally before the 4-layer pipeline. Any decision matching a categorical
//! exclusion routes directly to [`PipelineResult::UserEscalation`] — no canon check,
//! no Northstar check, no LightArchitect consultation.
//!
//! # Layer summary
//!
//! | Layer | Gate | Resolver |
//! |-------|------|---------|
//! | 0 | CategoricalExclusion | Hardcoded pattern table (ADR-002) |
//! | 1 | Canon | Static rule set (Phase 2); `PlatformClient` in Phase 4 |
//! | 2 | Northstar | Pillar regression check |
//! | 3 | LightArchitect | `LightArchitectRegistry` specialist routing |
//! | 4 | User | Escalation via `ironclaw-hitl` SSE channel (Phase 4) |
//!
//! [`DecisionPipeline::evaluate`] is synchronous in Phase 2. Phase 4 replaces the
//! canon and LightArchitect layers with async `PlatformClient` calls while keeping
//! this module's public API stable.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::lightsquad::decisions::hash_chain::DecisionLayer;

// ─── Errors ───────────────────────────────────────────────────────────────────

/// Errors produced by pipeline evaluation.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// A required context field was missing or malformed.
    #[error("invalid decision context: {0}")]
    InvalidContext(String),
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, PipelineError>;

// ─── ActionKind ───────────────────────────────────────────────────────────────

/// The category of action the worker is requesting permission for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Writing or modifying a file.
    FileWrite,
    /// Deleting a file or directory.
    FileDelete,
    /// Adding a new dependency to `Cargo.toml`.
    DependencyAdd {
        /// Name of the crate being added.
        dep_name: String,
    },
    /// Spawning a subprocess.
    ProcessExec {
        /// Full command string (program + args).
        command: String,
    },
    /// Outbound network request.
    NetworkRequest {
        /// Target hostname (without scheme or port).
        host: String,
    },
    /// Any `git` repository mutation (commit, reset, branch ops).
    GitOperation {
        /// Human-readable description of the operation.
        description: String,
    },
    /// Introducing an `unsafe` block.
    UnsafeCode {
        /// Source location (`file:line`).
        location: String,
    },
    /// FFI extern call.
    FfiCall {
        /// Symbol name being called (e.g. `libc::fork`).
        symbol: String,
    },
    /// Any other action not covered above.
    Other(String),
}

// ─── DecisionContext ──────────────────────────────────────────────────────────

/// Everything the pipeline needs to evaluate a decision request.
///
/// Built by the worker and sent through the `ironclaw-hitl` channel to the
/// Supervisor. All fields must be non-empty for the pipeline to evaluate correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionContext {
    /// Unique task identifier that originated this request.
    pub task_id: String,
    /// Human-readable description of the decision (≤ 512 chars).
    pub description: String,
    /// The category of action being requested.
    pub action_kind: ActionKind,
    /// Absolute file paths involved in the action (may be empty for non-file actions).
    pub file_paths: Vec<PathBuf>,
}

impl DecisionContext {
    /// Validate that required fields are present and within bounds.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::InvalidContext`] if `task_id` is empty or
    /// `description` exceeds 512 chars.
    pub fn validate(&self) -> Result<()> {
        if self.task_id.is_empty() {
            return Err(PipelineError::InvalidContext(
                "task_id must not be empty".to_owned(),
            ));
        }
        if self.description.len() > 512 {
            return Err(PipelineError::InvalidContext(
                "description must not exceed 512 chars".to_owned(),
            ));
        }
        Ok(())
    }
}

// ─── CategoricalExclusion ────────────────────────────────────────────────────

/// Layer 0 pre-screen — unconditionally routes matching decisions to the operator.
///
/// Defined in ADR-002. Any variant here bypasses all four pipeline layers and
/// goes directly to [`PipelineResult::UserEscalation`]. This means even a
/// canon-compliant, Northstar-aligned action will be escalated if it falls into
/// a categorical exclusion zone.
///
/// # Rationale (OWASP LLM01 / Security Guardrails §6.1)
///
/// An LLM worker could craft a prompt that tricks Layers 1–3 into auto-approving
/// a destructive operation. Layer 0 is the hardcoded safety net that cannot be
/// reasoned around.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CategoricalExclusion {
    /// Destructive filesystem operation (`rm -rf`, truncate, overwrite outside `file_ownership`).
    DestructiveOp {
        /// Human-readable description of the destructive operation.
        description: String,
    },
    /// Any file path touching secrets: `.env`, `.ssh/`, `*.pem`, `*.key`, `secrets.*`.
    SecretTouching {
        /// Absolute path of the secret file being accessed.
        path: String,
    },
    /// Net-new `[dependencies]` or `[patch]` addition to `Cargo.toml` (supply chain gate).
    DepAddition {
        /// Name of the dependency being added.
        dep_name: String,
    },
    /// `unsafe` block introduced outside an existing `unsafe` context.
    UnsafeBlock {
        /// Source location where the `unsafe` block appears (`file:line`).
        location: String,
    },
    /// FFI `extern "C"` call.
    FfiCall {
        /// Symbol name being called via FFI.
        symbol: String,
    },
    /// Network egress to a host outside the declared SSRF allowlist.
    NetworkEgress {
        /// Target hostname that is not allowlisted.
        host: String,
    },
    /// Irreversible migration: `DROP TABLE`, `DELETE` without `WHERE`, schema migration.
    IrreversibleMigration {
        /// Description of the irreversible operation.
        operation: String,
    },
}

impl CategoricalExclusion {
    /// Return `Some(exclusion)` if `ctx` matches any categorical exclusion zone.
    ///
    /// Returns `None` when the decision is safe to pass through the 4-layer pipeline.
    #[must_use]
    pub fn screen(ctx: &DecisionContext) -> Option<Self> {
        // ── FileDelete → DestructiveOp ──
        if matches!(ctx.action_kind, ActionKind::FileDelete) {
            return Some(Self::DestructiveOp {
                description: ctx.description.clone(),
            });
        }

        // ── File paths containing secrets ──
        for path in &ctx.file_paths {
            if let Some(exc) = Self::check_secret_path(path) {
                return Some(exc);
            }
        }

        // ── DependencyAdd ──
        if let ActionKind::DependencyAdd { dep_name } = &ctx.action_kind {
            return Some(Self::DepAddition {
                dep_name: dep_name.clone(),
            });
        }

        // ── UnsafeCode ──
        if let ActionKind::UnsafeCode { location } = &ctx.action_kind {
            return Some(Self::UnsafeBlock {
                location: location.clone(),
            });
        }

        // ── FfiCall ──
        if let ActionKind::FfiCall { symbol } = &ctx.action_kind {
            return Some(Self::FfiCall {
                symbol: symbol.clone(),
            });
        }

        // ── NetworkRequest to unlisted host ──
        if let ActionKind::NetworkRequest { host } = &ctx.action_kind {
            if !is_allowlisted_host(host) {
                return Some(Self::NetworkEgress { host: host.clone() });
            }
        }

        // ── FileWrite to Cargo.toml with dep-like description ──
        if matches!(ctx.action_kind, ActionKind::FileWrite) {
            for path in &ctx.file_paths {
                if path.file_name().is_some_and(|n| n == "Cargo.toml")
                    && ctx.description.contains("[dependencies]")
                {
                    return Some(Self::DepAddition {
                        dep_name: "unknown (Cargo.toml write)".to_owned(),
                    });
                }
            }
        }

        None
    }

    /// Human-readable description of why this exclusion applies.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::DestructiveOp { description } => {
                format!("destructive filesystem operation: {description}")
            }
            Self::SecretTouching { path } => format!("secret file access: {path}"),
            Self::DepAddition { dep_name } => {
                format!("dependency addition (supply chain gate): {dep_name}")
            }
            Self::UnsafeBlock { location } => {
                format!("unsafe block introduction: {location}")
            }
            Self::FfiCall { symbol } => format!("FFI extern call: {symbol}"),
            Self::NetworkEgress { host } => {
                format!("network egress to non-allowlisted host: {host}")
            }
            Self::IrreversibleMigration { operation } => {
                format!("irreversible migration: {operation}")
            }
        }
    }

    fn check_secret_path(path: &Path) -> Option<Self> {
        let path_str = path.to_string_lossy();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy())
            .unwrap_or_default();

        let is_secret = file_name == ".env"
            || file_name.starts_with(".env.")
            || path_str.contains("/.ssh/")
            || extension == "pem"
            || extension == "key"
            || file_name.starts_with("secrets.")
            || file_name == "ANTHROPIC_API_KEY"
            || path_str.contains("/.aws/credentials")
            || path_str.contains("/.gnupg/");

        if is_secret {
            Some(Self::SecretTouching {
                path: path_str.into_owned(),
            })
        } else {
            None
        }
    }
}

/// Returns `true` if `host` is on the static SSRF allowlist.
///
/// The allowlist covers the declared external endpoints for the ironclaw build:
/// Ollama Cloud API, AYIN HTTP dashboard (localhost), and the lightarchitects
/// platform API gateway.
fn is_allowlisted_host(host: &str) -> bool {
    const ALLOWLIST: &[&str] = &[
        "ollama.ai",
        "api.ollama.ai",
        "127.0.0.1",
        "localhost",
        "api.lightarchitects.io",
    ];
    ALLOWLIST
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

// ─── PipelineResult ───────────────────────────────────────────────────────────

/// The verdict produced by [`DecisionPipeline::evaluate`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum PipelineResult {
    /// The action is approved — proceed.
    Approved {
        /// Which layer resolved the decision.
        layer: DecisionLayer,
        /// Supporting citation (canon section, Northstar pillar, etc.).
        citation: Option<String>,
    },
    /// The action is blocked — do not proceed.
    Blocked {
        /// Why the action was blocked.
        reason: String,
        /// Which layer blocked it.
        layer: DecisionLayer,
        /// Supporting citation.
        citation: Option<String>,
    },
    /// The decision requires operator input.
    UserEscalation {
        /// Reason for escalation.
        reason: String,
        /// The categorical exclusion that triggered this, if applicable.
        exclusion: Option<CategoricalExclusion>,
    },
}

impl PipelineResult {
    /// Returns `true` if this result permits the action to proceed.
    #[must_use]
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved { .. })
    }

    /// Returns `true` if operator input is required before the action can proceed.
    #[must_use]
    pub fn requires_user(&self) -> bool {
        matches!(self, Self::UserEscalation { .. })
    }
}

// ─── DecisionPipeline ────────────────────────────────────────────────────────

/// Evaluates [`DecisionContext`] through the 4-layer pipeline.
///
/// Phase 2 uses a static rule set for Layers 1–3. Phase 4 replaces these with
/// async `PlatformClient` calls while keeping this struct's public API unchanged.
#[derive(Debug, Default)]
pub struct DecisionPipeline;

impl DecisionPipeline {
    /// Create a new pipeline instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Evaluate `ctx` through all pipeline layers and return a verdict.
    ///
    /// # Layer sequence
    ///
    /// 1. Layer 0 — [`CategoricalExclusion::screen`]: if a match is found,
    ///    immediately return [`PipelineResult::UserEscalation`].
    /// 2. Layer 1 — Canon: static rule check against core canon principles.
    /// 3. Layer 2 — Northstar: pillar regression check.
    /// 4. Layer 3 — `LightArchitect`: default domain approval (Phase 2 stub;
    ///    Phase 4 dispatches to `LightArchitectRegistry`).
    #[must_use]
    pub fn evaluate(&self, ctx: &DecisionContext) -> PipelineResult {
        // Layer 0: categorical exclusion pre-screen (ADR-002).
        if let Some(exc) = CategoricalExclusion::screen(ctx) {
            return PipelineResult::UserEscalation {
                reason: exc.description(),
                exclusion: Some(exc),
            };
        }

        // Layer 1: canon check (Phase 2 static rules).
        if let Some(result) = Self::canon_check(ctx) {
            return result;
        }

        // Layer 2: Northstar pillar regression check.
        if let Some(result) = Self::northstar_check(ctx) {
            return result;
        }

        // Layer 3: LightArchitect domain approval (Phase 2 default — always approves
        // actions that cleared Layers 0–2; Phase 4 adds specialist routing).
        PipelineResult::Approved {
            layer: DecisionLayer::LightArchitect,
            citation: Some("Phase 2 default: cleared L0-L2".to_owned()),
        }
    }

    // ── Private layer implementations ────────────────────────────────────────

    fn canon_check(ctx: &DecisionContext) -> Option<PipelineResult> {
        // Canon XIV §3 — no force-push or history rewrite on shared branches.
        if let ActionKind::GitOperation { description } = &ctx.action_kind {
            let lower = description.to_lowercase();
            if lower.contains("force-push")
                || lower.contains("--force")
                || lower.contains("reset --hard")
            {
                return Some(PipelineResult::Blocked {
                    reason:
                        "Canon XIV §3: force-push and history rewrite forbidden on shared branches"
                            .to_owned(),
                    layer: DecisionLayer::Canon,
                    citation: Some("canon://platform-canon#canon-xiv".to_owned()),
                });
            }
        }

        // Canon V — no speculation stated as fact (non-applicable at this layer).
        // Canon XXX — strand mosaic; all 10 dimensions must have a gate home
        // (checked at plan review time, not runtime).
        None
    }

    fn northstar_check(ctx: &DecisionContext) -> Option<PipelineResult> {
        // Northstar P2 mechanical check 3: no action may cause the autonomous
        // delivery loop to block indefinitely (deadlock / infinite sleep).
        if let ActionKind::ProcessExec { command } = &ctx.action_kind {
            let lower = command.to_lowercase();
            if lower.contains("sleep infinity") || lower.contains("tail -f /dev/null") {
                return Some(PipelineResult::Blocked {
                    reason: "Northstar P2 check 3: action would block the autonomous delivery loop indefinitely".to_owned(),
                    layer: DecisionLayer::Northstar,
                    citation: Some("canon://northstar#p2-check-3".to_owned()),
                });
            }
        }
        None
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ctx(kind: ActionKind, paths: Vec<PathBuf>) -> DecisionContext {
        DecisionContext {
            task_id: "task-001".to_owned(),
            description: "test action".to_owned(),
            action_kind: kind,
            file_paths: paths,
        }
    }

    // ── Layer 0: CategoricalExclusion ────────────────────────────────────────

    #[test]
    fn file_delete_is_destructive_op() {
        let ctx = ctx(ActionKind::FileDelete, vec![]);
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(matches!(
            exc,
            Some(CategoricalExclusion::DestructiveOp { .. })
        ));
    }

    #[test]
    fn dep_add_escalates() {
        let ctx = ctx(
            ActionKind::DependencyAdd {
                dep_name: "serde-hack".to_owned(),
            },
            vec![],
        );
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(matches!(
            exc,
            Some(CategoricalExclusion::DepAddition { .. })
        ));
    }

    #[test]
    fn unsafe_code_escalates() {
        let ctx = ctx(
            ActionKind::UnsafeCode {
                location: "src/foo.rs:42".to_owned(),
            },
            vec![],
        );
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(matches!(
            exc,
            Some(CategoricalExclusion::UnsafeBlock { .. })
        ));
    }

    #[test]
    fn ffi_call_escalates() {
        let ctx = ctx(
            ActionKind::FfiCall {
                symbol: "libc::fork".to_owned(),
            },
            vec![],
        );
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(matches!(exc, Some(CategoricalExclusion::FfiCall { .. })));
    }

    #[test]
    fn secret_file_env_escalates() {
        let ctx = ctx(
            ActionKind::FileWrite,
            vec![PathBuf::from("/home/user/.env")],
        );
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(matches!(
            exc,
            Some(CategoricalExclusion::SecretTouching { .. })
        ));
    }

    #[test]
    fn pem_file_escalates() {
        let ctx = ctx(
            ActionKind::FileWrite,
            vec![PathBuf::from("/etc/ssl/server.pem")],
        );
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(matches!(
            exc,
            Some(CategoricalExclusion::SecretTouching { .. })
        ));
    }

    #[test]
    fn allowlisted_network_host_passes() {
        let ctx = ctx(
            ActionKind::NetworkRequest {
                host: "api.ollama.ai".to_owned(),
            },
            vec![],
        );
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(
            exc.is_none(),
            "allowlisted host should not trigger exclusion"
        );
    }

    #[test]
    fn unlisted_network_host_escalates() {
        let ctx = ctx(
            ActionKind::NetworkRequest {
                host: "evil.example.com".to_owned(),
            },
            vec![],
        );
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(matches!(
            exc,
            Some(CategoricalExclusion::NetworkEgress { .. })
        ));
    }

    #[test]
    fn safe_file_write_passes_layer0() {
        let ctx = ctx(ActionKind::FileWrite, vec![PathBuf::from("src/lib.rs")]);
        let exc = CategoricalExclusion::screen(&ctx);
        assert!(exc.is_none());
    }

    // ── Layer 1: Canon ───────────────────────────────────────────────────────

    #[test]
    fn force_push_blocked_by_canon() {
        let pipeline = DecisionPipeline::new();
        let ctx = ctx(
            ActionKind::GitOperation {
                description: "git push --force origin main".to_owned(),
            },
            vec![],
        );
        let result = pipeline.evaluate(&ctx);
        assert!(matches!(
            result,
            PipelineResult::Blocked {
                layer: DecisionLayer::Canon,
                ..
            }
        ));
    }

    // ── Layer 3: Default approval ────────────────────────────────────────────

    #[test]
    fn safe_file_write_approved_by_pipeline() {
        let pipeline = DecisionPipeline::new();
        let ctx = ctx(ActionKind::FileWrite, vec![PathBuf::from("src/foo.rs")]);
        let result = pipeline.evaluate(&ctx);
        assert!(result.is_approved());
    }

    #[test]
    fn process_exec_non_blocking_approved() {
        let pipeline = DecisionPipeline::new();
        let ctx = ctx(
            ActionKind::ProcessExec {
                command: "cargo test --all-features".to_owned(),
            },
            vec![],
        );
        let result = pipeline.evaluate(&ctx);
        assert!(result.is_approved());
    }

    #[test]
    fn infinite_sleep_blocked_by_northstar() {
        let pipeline = DecisionPipeline::new();
        let ctx = ctx(
            ActionKind::ProcessExec {
                command: "sleep infinity".to_owned(),
            },
            vec![],
        );
        let result = pipeline.evaluate(&ctx);
        assert!(matches!(
            result,
            PipelineResult::Blocked {
                layer: DecisionLayer::Northstar,
                ..
            }
        ));
    }

    // ── Validation ──────────────────────────────────────────────────────────

    #[test]
    fn empty_task_id_fails_validation() {
        let ctx = DecisionContext {
            task_id: String::new(),
            description: "test".to_owned(),
            action_kind: ActionKind::FileWrite,
            file_paths: vec![],
        };
        assert!(ctx.validate().is_err());
    }

    #[test]
    fn description_too_long_fails_validation() {
        let ctx = DecisionContext {
            task_id: "t".to_owned(),
            description: "a".repeat(513),
            action_kind: ActionKind::FileWrite,
            file_paths: vec![],
        };
        assert!(ctx.validate().is_err());
    }

    #[test]
    fn pipeline_result_is_approved_predicate() {
        assert!(
            PipelineResult::Approved {
                layer: DecisionLayer::Canon,
                citation: None,
            }
            .is_approved()
        );
        assert!(
            !PipelineResult::Blocked {
                reason: "blocked".to_owned(),
                layer: DecisionLayer::Canon,
                citation: None,
            }
            .is_approved()
        );
    }

    #[test]
    fn categorical_exclusion_description_non_empty() {
        let exc = CategoricalExclusion::DestructiveOp {
            description: "rm -rf /".to_owned(),
        };
        assert!(!exc.description().is_empty());
    }
}
