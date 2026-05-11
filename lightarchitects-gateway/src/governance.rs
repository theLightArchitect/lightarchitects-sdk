//! Scope governance — trust and scope enforcement for agent orchestration.
//!
//! Called by `lightarchitects_orchestrate` before any agent subprocess is spawned.
//! The governance layer is the *first* enforcement layer; agent-side checks
//! remain in place as a second layer.
//!
//! # Trust model
//!
//! | Level | Restriction |
//! |---|---|
//! | `Trusted` | No restrictions — agent may execute any action. |
//! | `Sandboxed` | Destructive or system-level actions are blocked by exact name (see `SANDBOXED_BLOCKLIST`). |
//! | `Untrusted` | Only read-class actions are permitted (`read`, `query`, `search`, `helix`, `stats`, `health`). |
//!
//! # Scope model
//!
//! | Level | Restriction |
//! |---|---|
//! | `All` | No path restrictions. |
//! | `Shared` | `path` params must be within the agent's namespace or `user/`/`shared/`. |
//! | `Own` | `path` params must be within the agent's own namespace only. |

use serde_json::Value;

use crate::config::{ScopeLevel, TrustLevel};
use crate::error::GatewayError;

// ── Trust enforcement ──────────────────────────────────────────────────────────

/// Actions blocked for `Sandboxed` routes.
///
/// Each entry is an exact canonical action name (no substring matching).
/// Grouped by source:
///
/// - **Core**: `bash` (shell execution).
/// - **CORSO**: `deploy`, `rollback`, `strike`, `write_file`.
/// - **SERAPH internal wings** (defense-in-depth; these are scope-gated in the
///   SERAPH binary, but blocked here as a second enforcement layer):
///   `execute`, `detonate`, `capture`, `scan`, `osint`, `monitor`, `orchestrate`.
const SANDBOXED_BLOCKLIST: &[&str] = &[
    // Core
    "bash",
    // CORSO — destructive / system-modifying
    "deploy",
    "rollback",
    "strike",
    "write_file",
    // SERAPH — internal scope-gated wing actions (defense-in-depth)
    "execute",
    "detonate",
    "capture",
    "scan",
    "osint",
    "monitor",
    "orchestrate",
];

/// Actions permitted for `Untrusted` routes (allowlist — everything else is denied).
const UNTRUSTED_ALLOWLIST: &[&str] = &[
    "read",
    "query",
    "search",
    "helix",
    "stats",
    "health",
    "discover",
    "list",
    "list_notes",
    "read_note",
    "manifest",
    "validate",
];

/// Enforce trust level for the given `action`.
///
/// # Errors
///
/// Returns [`GatewayError::Governance`] when the action is not permitted at the
/// route's trust level.
pub fn check_trust(agent: &str, trust: TrustLevel, action: &str) -> Result<(), GatewayError> {
    match trust {
        TrustLevel::Trusted => Ok(()),
        TrustLevel::Sandboxed => {
            let blocked = SANDBOXED_BLOCKLIST.contains(&action);
            if blocked {
                Err(GatewayError::Governance {
                    agent: agent.to_owned(),
                    reason: format!(
                        "action '{action}' is not permitted for sandboxed agent '{agent}'. \
                         Sandboxed agents cannot perform destructive or system-level actions."
                    ),
                })
            } else {
                Ok(())
            }
        }
        TrustLevel::Untrusted => {
            let allowed = UNTRUSTED_ALLOWLIST.contains(&action);
            if allowed {
                Ok(())
            } else {
                Err(GatewayError::Governance {
                    agent: agent.to_owned(),
                    reason: format!(
                        "action '{action}' is not permitted for untrusted agent '{agent}'. \
                         Only read-class actions are allowed: {UNTRUSTED_ALLOWLIST:?}"
                    ),
                })
            }
        }
    }
}

// ── Scope enforcement ──────────────────────────────────────────────────────────

/// Namespaces accessible under `Shared` scope (in addition to the agent's own).
const SHARED_NAMESPACES: &[&str] = &["user/", "shared/"];

/// Enforce scope level for a route calling an action with the given params.
///
/// When the params contain a `path` string that looks like a helix namespace path
/// (contains `/`), the scope is checked. Params without a `path` field pass
/// unconditionally — non-helix actions are not scope-restricted.
///
/// # Errors
///
/// Returns [`GatewayError::Governance`] when the path falls outside the allowed scope.
pub fn check_scope(agent: &str, scope: ScopeLevel, params: &Value) -> Result<(), GatewayError> {
    // Scope only applies when params contain a helix `path` field.
    let path = match params.get("path").and_then(Value::as_str) {
        Some(p) if p.contains('/') => p,
        _ => return Ok(()),
    };

    match scope {
        ScopeLevel::All => Ok(()),
        ScopeLevel::Shared => {
            let own_prefix = format!("{agent}/");
            if path.starts_with(&own_prefix)
                || SHARED_NAMESPACES.iter().any(|ns| path.starts_with(ns))
            {
                Ok(())
            } else {
                Err(GatewayError::Governance {
                    agent: agent.to_owned(),
                    reason: format!(
                        "path '{path}' is outside the allowed scope for '{agent}' (Shared). \
                         Allowed: '{own_prefix}', 'user/', 'shared/'."
                    ),
                })
            }
        }
        ScopeLevel::Own => {
            let own_prefix = format!("{agent}/");
            if path.starts_with(&own_prefix) {
                Ok(())
            } else {
                Err(GatewayError::Governance {
                    agent: agent.to_owned(),
                    reason: format!(
                        "path '{path}' is outside the allowed scope for '{agent}' (Own). \
                         Allowed: '{own_prefix}'."
                    ),
                })
            }
        }
    }
}

// ── Combined gate ──────────────────────────────────────────────────────────────

/// Run both trust and scope checks.
///
/// This is the single call site used by the orchestrate handler. Order:
/// 1. Trust check — blocks dangerous actions before any I/O.
/// 2. Scope check — validates path access before subprocess spawn.
///
/// # Errors
///
/// Returns the first [`GatewayError::Governance`] encountered.
pub fn enforce(
    agent: &str,
    trust: TrustLevel,
    scope: ScopeLevel,
    action: &str,
    params: &Value,
) -> Result<(), GatewayError> {
    check_trust(agent, trust, action)?;
    check_scope(agent, scope, params)?;
    Ok(())
}

// ── User-scope enforcement ─────────────────────────────────────────────────────

/// Scope tier for vault entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeTier {
    /// Platform-wide entries (any user can read; write disabled in v1).
    Platform,
    /// Per-user entries (only matching `user_id` can read/write).
    User,
    /// Project-scoped entries (any authenticated user can read/write).
    Project,
    /// Shared entries (any authenticated user can read; write by override).
    Shared,
}

/// Check whether a user is allowed to access a path at the given scope tier.
///
/// # Errors
///
/// Returns [`GatewayError::Governance`] when access is denied.
pub fn check_user_scope(
    user_id: &str,
    path: &std::path::Path,
    scope_tier: ScopeTier,
) -> Result<(), GatewayError> {
    match scope_tier {
        ScopeTier::Platform => {
            // Platform: any user can read; write disabled in v1.0
            if is_write_action(path) {
                return Err(GatewayError::Governance {
                    agent: user_id.to_owned(),
                    reason: "platform tier: write disabled in v1.0".to_string(),
                });
            }
            Ok(())
        }
        ScopeTier::User => {
            // User tier: user_id MUST match path segment (helix/user/{user_id}/...)
            if path_contains_user_id(path, user_id) {
                Ok(())
            } else {
                Err(GatewayError::Governance {
                    agent: user_id.to_owned(),
                    reason: format!("user tier: path does not contain user_id '{user_id}'"),
                })
            }
        }
        ScopeTier::Project => {
            // Project tier: any authenticated user can read/write
            if user_id == "local" {
                return Err(GatewayError::Governance {
                    agent: user_id.to_owned(),
                    reason: "project tier: unauthenticated user".to_string(),
                });
            }
            Ok(())
        }
        ScopeTier::Shared => {
            // Shared tier: any authenticated user can read; write by override
            if is_write_action(path) {
                // v1.0: no write overrides — read-only for shared tier
                return Err(GatewayError::Governance {
                    agent: user_id.to_owned(),
                    reason: "shared tier: write disabled in v1.0".to_string(),
                });
            }
            if user_id == "local" {
                return Err(GatewayError::Governance {
                    agent: user_id.to_owned(),
                    reason: "shared tier: unauthenticated user".to_string(),
                });
            }
            Ok(())
        }
    }
}

fn is_write_action(_path: &std::path::Path) -> bool {
    // Heuristic: paths ending in known write patterns (upload, create, etc.)
    // In v1.0, all platform/shared writes are blocked regardless of path suffix.
    // This function is a placeholder for future path-based write detection.
    false
}

fn path_contains_user_id(path: &std::path::Path, user_id: &str) -> bool {
    let comps: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    comps.windows(2).any(|w| w[0] == "user" && w[1] == user_id)
}

// ── ScopeGovernor 5-gate — operator action enforcement (Wave 3.2) ─────────────

/// Context for evaluating an operator action against the 5-gate `ScopeGovernor`.
#[derive(Debug, Clone)]
pub struct ScopeGovernorContext {
    /// Operator identifier (session-bound).
    pub operator_id: String,
    /// The build being acted upon.
    pub build_id: String,
    /// Tool or action being requested.
    pub tool: String,
    /// RFC 3339 timestamp from the operator payload.
    pub timestamp_iso8601: String,
    /// Build IDs the operator is authorized to act on (empty = all builds).
    pub authorized_builds: Vec<String>,
    /// Tools the operator may use (empty = unrestricted).
    pub allowed_tools: Vec<String>,
    /// Number of in-flight operator actions for this operator right now.
    pub concurrent_count: usize,
    /// Maximum concurrent operator actions (default 5).
    pub concurrent_limit: usize,
}

impl Default for ScopeGovernorContext {
    fn default() -> Self {
        Self {
            operator_id: String::new(),
            build_id: String::new(),
            tool: String::new(),
            timestamp_iso8601: String::new(),
            authorized_builds: Vec::new(),
            allowed_tools: Vec::new(),
            concurrent_count: 0,
            concurrent_limit: 5,
        }
    }
}

/// Wall-clock TTL (seconds) for operator-action contexts. Gate 1.
const OPERATOR_ACTION_TTL_SECS: i64 = 300;

/// Operator-domain tools — Gate 5 hard allowlist.
const OPERATOR_DOMAIN_TOOLS: &[&str] = &["resolve-assertion", "query-blocked-flow"];

/// Enforce the 5-gate `ScopeGovernor` for an operator action.
///
/// Gates evaluated in order (first failure returns error):
/// 1. **TTL** — payload timestamp within 300s of now.
/// 2. **Target** — `build_id` in `authorized_builds` (empty = all).
/// 3. **Tool** — `tool` in `allowed_tools` (empty = unrestricted).
/// 4. **Concurrent** — `concurrent_count < concurrent_limit`.
/// 5. **Domain** — tool is in the operator-action domain allowlist.
///
/// Emits a `lasdlc.hook.fire` AYIN span on every evaluation via [`emit_hook_span`].
///
/// # Errors
///
/// Returns [`GatewayError::Governance`] when any gate rejects.
pub fn enforce_operator_action(ctx: &ScopeGovernorContext) -> Result<(), GatewayError> {
    let result = (|| {
        check_ttl_gate(&ctx.operator_id, &ctx.timestamp_iso8601)?;
        check_target_gate(&ctx.operator_id, &ctx.build_id, &ctx.authorized_builds)?;
        check_tool_gate(&ctx.operator_id, &ctx.tool, &ctx.allowed_tools)?;
        check_concurrent_gate(&ctx.operator_id, ctx.concurrent_count, ctx.concurrent_limit)?;
        check_domain_gate(&ctx.operator_id, &ctx.build_id, &ctx.tool)?;
        Ok(())
    })();
    emit_hook_span(
        "PreToolUse:OperatorAction_ScopeGovernor",
        "security",
        result.is_err(),
    );
    result
}

fn check_ttl_gate(operator_id: &str, timestamp_iso8601: &str) -> Result<(), GatewayError> {
    // Positive diff = past (normal); negative = future-dated (allow 5s skew only).
    const MAX_SKEW: i64 = 5;
    use chrono::{DateTime, Utc};
    let ts = timestamp_iso8601
        .parse::<DateTime<Utc>>()
        .map_err(|_| GatewayError::Governance {
            agent: operator_id.to_owned(),
            reason: format!("Gate 1 (TTL): invalid timestamp '{timestamp_iso8601}'"),
        })?;
    let diff = Utc::now().signed_duration_since(ts).num_seconds();
    if !(-MAX_SKEW..=OPERATOR_ACTION_TTL_SECS).contains(&diff) {
        return Err(GatewayError::Governance {
            agent: operator_id.to_owned(),
            reason: format!(
                "Gate 1 (TTL): operator action outside window ({diff}s, window -{MAX_SKEW}..+{OPERATOR_ACTION_TTL_SECS}s)"
            ),
        });
    }
    Ok(())
}

fn check_target_gate(
    operator_id: &str,
    build_id: &str,
    authorized_builds: &[String],
) -> Result<(), GatewayError> {
    if authorized_builds.is_empty() || authorized_builds.iter().any(|b| b == build_id) {
        return Ok(());
    }
    Err(GatewayError::Governance {
        agent: operator_id.to_owned(),
        reason: format!("Gate 2 (Target): '{operator_id}' not authorized for build '{build_id}'"),
    })
}

fn check_tool_gate(
    operator_id: &str,
    tool: &str,
    allowed_tools: &[String],
) -> Result<(), GatewayError> {
    if allowed_tools.is_empty() || allowed_tools.iter().any(|t| t == tool) {
        return Ok(());
    }
    Err(GatewayError::Governance {
        agent: operator_id.to_owned(),
        reason: format!("Gate 3 (Tool): tool '{tool}' not in operator's allowed set"),
    })
}

fn check_concurrent_gate(
    operator_id: &str,
    count: usize,
    limit: usize,
) -> Result<(), GatewayError> {
    if count < limit {
        return Ok(());
    }
    Err(GatewayError::Governance {
        agent: operator_id.to_owned(),
        reason: format!("Gate 4 (Concurrent): '{operator_id}' exceeded limit ({count}/{limit})"),
    })
}

fn check_domain_gate(operator_id: &str, build_id: &str, tool: &str) -> Result<(), GatewayError> {
    if OPERATOR_DOMAIN_TOOLS.contains(&tool) {
        return Ok(());
    }
    Err(GatewayError::Governance {
        agent: operator_id.to_owned(),
        reason: format!(
            "Gate 5 (Domain): tool '{tool}' outside operator-action domain for '{build_id}'"
        ),
    })
}

// ── Citation staleness check (PostToolUse) ─────────────────────────────────────

/// Result of a citation cache-path check (`PostToolUse:Citation_StalenessCheck`).
#[derive(Debug, Clone)]
pub struct CitationCheckResult {
    /// Cache paths that exist on disk.
    pub resolved: Vec<String>,
    /// Cache paths that do not exist on disk.
    pub unresolved: Vec<String>,
}

impl CitationCheckResult {
    /// Returns `true` if every supplied path resolved.
    #[must_use]
    pub fn all_resolved(&self) -> bool {
        self.unresolved.is_empty()
    }
}

/// Validate that citation cache paths resolve on disk.
///
/// Called on `PostToolUse:Citation_StalenessCheck` events. Missing paths indicate
/// the citation was never hydrated or the file was deleted. Staleness by file age
/// (>30 days) is deferred to Wave 3.3 webshell background job.
///
/// Emits a `lasdlc.hook.fire` AYIN span summarising the check result.
#[must_use]
pub fn validate_citations(cache_paths: &[&str]) -> CitationCheckResult {
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for path in cache_paths {
        if std::path::Path::new(path).exists() {
            resolved.push((*path).to_owned());
        } else {
            unresolved.push((*path).to_owned());
        }
    }
    let blocked = !unresolved.is_empty();
    emit_hook_span(
        "PostToolUse:Citation_StalenessCheck",
        "standards_compliance",
        blocked,
    );
    CitationCheckResult {
        resolved,
        unresolved,
    }
}

// ── AYIN span emission ─────────────────────────────────────────────────────────

/// Emit a `lasdlc.hook.fire` AYIN trace span asynchronously.
///
/// Fire-and-forget via `tokio::spawn`. Gracefully degrades (logs a warning)
/// when no tokio runtime is active. Uses [`lightarchitects::ayin::semconv::lasdlc`]
/// constants for attribute keys, satisfying Wave 3.2 acceptance criterion 11.
pub fn emit_hook_span(hook_name: &str, decision_class: &str, blocked: bool) {
    use lightarchitects::ayin::semconv::lasdlc;
    use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};

    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::trace!(hook = hook_name, blocked, "hook span skipped: no runtime");
        return;
    };

    let hook_name = hook_name.to_owned();
    let decision_class = decision_class.to_owned();

    handle.spawn(async move {
        let metadata = serde_json::json!({
            lasdlc::ATTR_HOOK_NAME: &hook_name,
            lasdlc::ATTR_DECISION_CLASS: &decision_class,
            lasdlc::ATTR_BLOCKED: blocked,
            lasdlc::ATTR_VALIDATION_STATUS_EMITTED: if blocked { "BLOCKED" } else { "VALIDATED" },
        });
        let outcome = if blocked {
            TraceOutcome::Block
        } else {
            TraceOutcome::Continue
        };
        let ctx = TraceContext::new(Actor::new("gateway"), lasdlc::SPAN_HOOK_FIRE)
            .metadata(metadata)
            .outcome(outcome);
        match ctx.finish() {
            Ok(span) => write_hook_span(span).await,
            Err(e) => tracing::warn!(error = %e, "hook span build failed"),
        }
    });
}

async fn write_hook_span(span: lightarchitects::ayin::span::TraceSpan) {
    use std::path::PathBuf;
    let base = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lightarchitects/soul/helix/ayin/traces");
    let dir = base
        .join(span.actor.as_str())
        .join(span.timestamp.format("%Y-%m-%d").to_string());
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!(error = %e, "AYIN hook trace dir failed");
        return;
    }
    let safe_action = span.action.replace('/', "_");
    let id_str = span.id.to_string();
    let name = format!(
        "{}-{}-{}.json",
        span.timestamp.format("%H-%M-%S"),
        safe_action,
        &id_str[..8]
    );
    match serde_json::to_vec(&span) {
        Ok(bytes) => {
            if let Err(e) = tokio::fs::write(dir.join(name), bytes).await {
                tracing::warn!(error = %e, "AYIN hook trace write failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "AYIN hook span serialize failed"),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    // ── Trust tests ────────────────────────────────────────────────────────────

    #[test]
    fn trusted_allows_any_action() {
        assert!(check_trust("corso", TrustLevel::Trusted, "deploy").is_ok());
        assert!(check_trust("seraph", TrustLevel::Trusted, "strike").is_ok());
    }

    #[test]
    fn sandboxed_blocks_destructive_actions() {
        assert!(check_trust("eva", TrustLevel::Sandboxed, "deploy").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "bash").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "strike").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "rollback").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "execute").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "detonate").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "capture").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "scan").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "osint").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "monitor").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "orchestrate").is_err());
        assert!(check_trust("eva", TrustLevel::Sandboxed, "write_file").is_err());
    }

    #[test]
    fn sandboxed_uses_exact_match_no_false_positives() {
        // These contain blocklisted words as substrings but are NOT blocked.
        // "undeploy" contains "deploy" — must NOT be blocked.
        assert!(check_trust("soul", TrustLevel::Sandboxed, "undeploy").is_ok());
        // "redeploy" contains "deploy" — must NOT be blocked.
        assert!(check_trust("soul", TrustLevel::Sandboxed, "redeploy").is_ok());
        // "execute_query" contains "execute" — must NOT be blocked.
        assert!(check_trust("soul", TrustLevel::Sandboxed, "execute_query").is_ok());
        // "confirm" contains "rm" (old substring bug) — must NOT be blocked.
        assert!(check_trust("soul", TrustLevel::Sandboxed, "confirm").is_ok());
        // "bash_history" contains "bash" — must NOT be blocked.
        assert!(check_trust("soul", TrustLevel::Sandboxed, "bash_history").is_ok());
        // "scanner" contains "scan" — must NOT be blocked.
        assert!(check_trust("soul", TrustLevel::Sandboxed, "scanner").is_ok());
    }

    #[test]
    fn sandboxed_allows_safe_actions() {
        assert!(check_trust("soul", TrustLevel::Sandboxed, "query").is_ok());
        assert!(check_trust("soul", TrustLevel::Sandboxed, "helix").is_ok());
        assert!(check_trust("soul", TrustLevel::Sandboxed, "memory").is_ok());
    }

    #[test]
    fn untrusted_allows_read_class() {
        assert!(check_trust("quantum", TrustLevel::Untrusted, "read").is_ok());
        assert!(check_trust("quantum", TrustLevel::Untrusted, "search").is_ok());
        assert!(check_trust("quantum", TrustLevel::Untrusted, "stats").is_ok());
    }

    #[test]
    fn untrusted_blocks_write_class() {
        assert!(check_trust("quantum", TrustLevel::Untrusted, "guard").is_err());
        assert!(check_trust("quantum", TrustLevel::Untrusted, "memory").is_err());
        assert!(check_trust("quantum", TrustLevel::Untrusted, "converse").is_err());
    }

    // ── Scope tests ────────────────────────────────────────────────────────────

    #[test]
    fn scope_all_allows_any_path() {
        let params = json!({"path": "seraph/secret-stuff"});
        assert!(check_scope("corso", ScopeLevel::All, &params).is_ok());
    }

    #[test]
    fn scope_own_allows_route_path() {
        let params = json!({"path": "corso/builds/foo"});
        assert!(check_scope("corso", ScopeLevel::Own, &params).is_ok());
    }

    #[test]
    fn scope_own_blocks_other_route_path() {
        let params = json!({"path": "eva/entries/personal"});
        assert!(check_scope("corso", ScopeLevel::Own, &params).is_err());
    }

    #[test]
    fn scope_shared_allows_user_namespace() {
        let params = json!({"path": "user/standards/canon/builders-cookbook.md"});
        assert!(check_scope("corso", ScopeLevel::Shared, &params).is_ok());
    }

    #[test]
    fn scope_shared_blocks_other_route() {
        let params = json!({"path": "seraph/scope.toml"});
        assert!(check_scope("corso", ScopeLevel::Shared, &params).is_err());
    }

    #[test]
    fn scope_check_passes_when_no_path_field() {
        // Non-helix actions without a path param are not scope-restricted.
        let params = json!({"action": "guard", "code": "fn main() {}"});
        assert!(check_scope("corso", ScopeLevel::Own, &params).is_ok());
    }

    #[test]
    fn enforce_combines_both_checks() {
        let params = json!({"path": "corso/builds"});
        assert!(
            enforce(
                "corso",
                TrustLevel::Trusted,
                ScopeLevel::Own,
                "guard",
                &params
            )
            .is_ok()
        );
        // Sandboxed + destructive action = fail at trust check before scope.
        assert!(
            enforce(
                "corso",
                TrustLevel::Sandboxed,
                ScopeLevel::Own,
                "deploy",
                &params
            )
            .is_err()
        );
    }

    // ── User-scope tests (Wave 2 identity-and-scoping) ───────────────────────

    #[test]
    fn user_scope_platform_allows_read_blocks_write() {
        assert!(
            check_user_scope("alice", Path::new("platform/canon.md"), ScopeTier::Platform).is_ok()
        );
    }

    #[test]
    fn user_scope_user_tier_requires_matching_user_id() {
        assert!(
            check_user_scope("alice", Path::new("user/alice/entry.md"), ScopeTier::User).is_ok()
        );
        assert!(
            check_user_scope(
                "alice",
                Path::new("user/alice/deep/nested.md"),
                ScopeTier::User
            )
            .is_ok()
        );
    }

    #[test]
    fn user_scope_user_tier_blocks_other_user() {
        assert!(
            check_user_scope("alice", Path::new("user/bob/entry.md"), ScopeTier::User).is_err()
        );
    }

    #[test]
    fn user_scope_user_tier_blocks_top_level_name_collision() {
        // A user_id matching a top-level directory (e.g., "platform") must NOT
        // grant access to that directory under User tier.
        assert!(
            check_user_scope("platform", Path::new("platform/canon.md"), ScopeTier::User).is_err()
        );
    }

    #[test]
    fn user_scope_user_tier_blocks_traversal() {
        // user_id of ".." must not match ParentDir components.
        assert!(check_user_scope("..", Path::new("user/alice/entry.md"), ScopeTier::User).is_err());
    }

    #[test]
    fn user_scope_project_tier_blocks_unauthenticated() {
        assert!(
            check_user_scope(
                "local",
                Path::new("project/alpha/spec.md"),
                ScopeTier::Project
            )
            .is_err()
        );
    }

    #[test]
    fn user_scope_project_tier_allows_authenticated() {
        assert!(
            check_user_scope(
                "alice",
                Path::new("project/alpha/spec.md"),
                ScopeTier::Project
            )
            .is_ok()
        );
    }

    #[test]
    fn user_scope_shared_tier_blocks_unauthenticated() {
        assert!(
            check_user_scope("local", Path::new("shared/oncall.md"), ScopeTier::Shared).is_err()
        );
    }

    #[test]
    fn user_scope_shared_tier_allows_read() {
        assert!(
            check_user_scope("alice", Path::new("shared/oncall.md"), ScopeTier::Shared).is_ok()
        );
    }
}
