//! Scope governance — trust and scope enforcement for sibling orchestration.
//!
//! Called by `lightarchitects_orchestrate` before any sibling subprocess is spawned.
//! The governance layer is the *first* enforcement layer; sibling-side checks
//! remain in place as a second layer.
//!
//! # Trust model
//!
//! | Level | Restriction |
//! |---|---|
//! | `Trusted` | No restrictions — sibling may execute any action. |
//! | `Sandboxed` | Destructive or system-level actions are blocked (`bash`, `deploy`, `pentest`, `strike`, `exploit`, `execute`, `rm`). |
//! | `Untrusted` | Only read-class actions are permitted (`read`, `query`, `search`, `helix`, `stats`, `health`). |
//!
//! # Scope model
//!
//! | Level | Restriction |
//! |---|---|
//! | `All` | No path restrictions. |
//! | `Shared` | `path` params must be within the sibling's namespace or `user/`/`shared/`. |
//! | `Own` | `path` params must be within the sibling's own namespace only. |

use serde_json::Value;

use crate::config::{ScopeLevel, TrustLevel};
use crate::error::GatewayError;

// ── Trust enforcement ──────────────────────────────────────────────────────────

/// Actions blocked for `Sandboxed` siblings.
///
/// These are system-modifying or potentially destructive action keywords.
const SANDBOXED_BLOCKLIST: &[&str] = &[
    "bash", "deploy", "pentest", "strike", "exploit", "execute", "rm", "delete", "drop",
];

/// Actions permitted for `Untrusted` siblings (allowlist — everything else is denied).
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
/// sibling's trust level.
pub fn check_trust(sibling: &str, trust: TrustLevel, action: &str) -> Result<(), GatewayError> {
    match trust {
        TrustLevel::Trusted => Ok(()),
        TrustLevel::Sandboxed => {
            let blocked = SANDBOXED_BLOCKLIST
                .iter()
                .any(|&blocked_kw| action.contains(blocked_kw));
            if blocked {
                Err(GatewayError::Governance {
                    sibling: sibling.to_owned(),
                    reason: format!(
                        "action '{action}' is not permitted for sandboxed sibling '{sibling}'. \
                         Sandboxed siblings cannot perform destructive or system-level actions."
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
                    sibling: sibling.to_owned(),
                    reason: format!(
                        "action '{action}' is not permitted for untrusted sibling '{sibling}'. \
                         Only read-class actions are allowed: {UNTRUSTED_ALLOWLIST:?}"
                    ),
                })
            }
        }
    }
}

// ── Scope enforcement ──────────────────────────────────────────────────────────

/// Namespaces accessible under `Shared` scope (in addition to the sibling's own).
const SHARED_NAMESPACES: &[&str] = &["user/", "shared/"];

/// Enforce scope level for a sibling calling an action with the given params.
///
/// When the params contain a `path` string that looks like a helix namespace path
/// (contains `/`), the scope is checked. Params without a `path` field pass
/// unconditionally — non-helix actions are not scope-restricted.
///
/// # Errors
///
/// Returns [`GatewayError::Governance`] when the path falls outside the allowed scope.
pub fn check_scope(sibling: &str, scope: ScopeLevel, params: &Value) -> Result<(), GatewayError> {
    // Scope only applies when params contain a helix `path` field.
    let path = match params.get("path").and_then(Value::as_str) {
        Some(p) if p.contains('/') => p,
        _ => return Ok(()),
    };

    match scope {
        ScopeLevel::All => Ok(()),
        ScopeLevel::Shared => {
            let own_prefix = format!("{sibling}/");
            if path.starts_with(&own_prefix)
                || SHARED_NAMESPACES.iter().any(|ns| path.starts_with(ns))
            {
                Ok(())
            } else {
                Err(GatewayError::Governance {
                    sibling: sibling.to_owned(),
                    reason: format!(
                        "path '{path}' is outside the allowed scope for '{sibling}' (Shared). \
                         Allowed: '{own_prefix}', 'user/', 'shared/'."
                    ),
                })
            }
        }
        ScopeLevel::Own => {
            let own_prefix = format!("{sibling}/");
            if path.starts_with(&own_prefix) {
                Ok(())
            } else {
                Err(GatewayError::Governance {
                    sibling: sibling.to_owned(),
                    reason: format!(
                        "path '{path}' is outside the allowed scope for '{sibling}' (Own). \
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
    sibling: &str,
    trust: TrustLevel,
    scope: ScopeLevel,
    action: &str,
    params: &Value,
) -> Result<(), GatewayError> {
    check_trust(sibling, trust, action)?;
    check_scope(sibling, scope, params)?;
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
    fn scope_own_allows_sibling_path() {
        let params = json!({"path": "corso/builds/foo"});
        assert!(check_scope("corso", ScopeLevel::Own, &params).is_ok());
    }

    #[test]
    fn scope_own_blocks_other_sibling_path() {
        let params = json!({"path": "eva/entries/personal"});
        assert!(check_scope("corso", ScopeLevel::Own, &params).is_err());
    }

    #[test]
    fn scope_shared_allows_user_namespace() {
        let params = json!({"path": "user/standards/builders-cookbook.md"});
        assert!(check_scope("corso", ScopeLevel::Shared, &params).is_ok());
    }

    #[test]
    fn scope_shared_blocks_other_sibling() {
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
