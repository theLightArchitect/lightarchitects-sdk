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

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let params = json!({"path": "user/standards/builders-cookbook.md"});
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
}
