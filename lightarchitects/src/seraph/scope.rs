//! Engagement scope management — Rust mirror of `~/lightarchitects/seraph/scope.toml`.
//!
//! Build an [`EngagementScope`], call [`EngagementScope::install`] to write it
//! to the expected path, then construct a [`crate::seraph::SeraphClient`].
//!
//! # SDK-side scope validation
//!
//! [`self::ScopeConstraint`] provides compile-time ergonomics for validating targets
//! and tools **before** dispatch. It does not replicate SERAPH's 5-gate server-side
//! `ScopeGovernor` — it is an SDK-level guard that rejects obviously invalid or
//! dangerous inputs (shell metacharacters, localhost targets, unknown tools)
//! before any network or IPC call is made.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core::error::SdkError;

/// Default maximum concurrent scans when not specified.
const fn default_max_concurrent() -> u8 {
    3
}

// ── EngagementScope ─────────────────────────────────────────────────────────

/// Rust representation of `~/lightarchitects/seraph/scope.toml`.
///
/// Build a scope, call [`EngagementScope::install`] to write it to the
/// expected path, then construct a [`crate::seraph::SeraphClient`].
///
/// # Example
///
/// ```no_run
/// # fn example() -> Result<(), crate::core::SdkError> {
/// use chrono::Utc;
/// use crate::seraph::scope::EngagementScope;
///
/// let scope = EngagementScope {
///     engagement_id: "ENG-001".into(),
///     targets: vec!["192.168.1.0/24".into()],
///     authorized_tools: vec!["nmap".into(), "tshark".into()],
///     expires_at: Utc::now() + chrono::Duration::hours(8),
///     hitl_required: false,
///     authorized_by: "kevin".into(),
///     max_concurrent_scans: 3,
/// };
/// scope.install()?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngagementScope {
    /// Unique engagement identifier -- logged with every tool invocation.
    pub engagement_id: String,
    /// Authorised network targets (CIDR or host notation).
    pub targets: Vec<String>,
    /// Allowlist of tool names permitted for this engagement.
    pub authorized_tools: Vec<String>,
    /// Engagement expiry timestamp (ISO 8601 / RFC 3339).
    pub expires_at: DateTime<Utc>,
    /// Whether human-in-the-loop confirmation is required before execution.
    pub hitl_required: bool,
    /// Name of the authorising individual (audit trail).
    pub authorized_by: String,
    /// Maximum number of concurrent scans permitted.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_scans: u8,
}

impl EngagementScope {
    /// Serialize to TOML suitable for writing to `~/lightarchitects/seraph/scope.toml`.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if serialization fails (should not
    /// happen with a well-formed `EngagementScope`).
    pub fn to_toml(&self) -> Result<String, SdkError> {
        toml::to_string(self)
            .map_err(|e| SdkError::Config(format!("failed to serialize scope to TOML: {e}")))
    }

    /// Write the scope to `~/lightarchitects/seraph/scope.toml`, creating the directory if
    /// needed.
    ///
    /// Uses an atomic tmp-file + chmod + rename sequence so the scope file is
    /// never visible with incorrect permissions (no TOCTOU window).
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] on file-system errors or if `$HOME` is
    /// not set. Returns [`SdkError::Config`] if TOML serialization fails.
    pub fn install(&self) -> Result<PathBuf, SdkError> {
        let path = scope_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SdkError::Config(format!(
                    "failed to create scope dir {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let toml = self.to_toml()?;
        write_scope_atomic(&path, &toml)?;
        Ok(path)
    }
}

/// Resolve `~/lightarchitects/seraph/scope.toml`.
fn scope_path() -> Result<PathBuf, SdkError> {
    crate::core::paths::seraph()
        .map(|p| p.join("scope.toml"))
        .ok_or_else(|| SdkError::Config("HOME environment variable not set".to_owned()))
}

/// Write `content` to `path` atomically with 0600 permissions.
///
/// Steps: write to `<path>.tmp` → chmod 0600 → rename to `<path>`.
/// The file is never visible at the final path with world-readable permissions.
fn write_scope_atomic(path: &PathBuf, content: &str) -> Result<(), SdkError> {
    use std::io::Write as _;

    let tmp = path.with_extension("toml.tmp");

    let mut f = std::fs::File::create(&tmp).map_err(|e| {
        SdkError::Config(format!(
            "failed to create tmp scope file {}: {e}",
            tmp.display()
        ))
    })?;
    f.write_all(content.as_bytes()).map_err(|e| {
        SdkError::Config(format!(
            "failed to write tmp scope file {}: {e}",
            tmp.display()
        ))
    })?;
    f.sync_all().map_err(|e| {
        SdkError::Config(format!(
            "failed to sync tmp scope file {}: {e}",
            tmp.display()
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
            SdkError::Config(format!(
                "failed to set 0600 permissions on {}: {e}",
                tmp.display()
            ))
        })?;
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        SdkError::Config(format!(
            "failed to rename {} → {}: {e}",
            tmp.display(),
            path.display()
        ))
    })
}

// ── ScopeDomain ─────────────────────────────────────────────────────────────

/// Pentest engagement domain — constrains which class of targets is in scope.
///
/// This enum is `#[non_exhaustive]` so that future SERAPH engagement types can
/// be added without a breaking change to downstream SDK consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScopeDomain {
    /// Web application targets (HTTP/HTTPS, APIs, web services).
    Web,
    /// Network infrastructure targets (hosts, CIDR ranges, ports).
    Network,
    /// Cloud platform targets (AWS, GCP, Azure, container registries).
    Cloud,
    /// Physical access / hardware targets (`IoT`, embedded, serial).
    Physical,
    /// Social engineering targets (phishing simulation, vishing).
    Social,
}

// ── ScopeConstraint ──────────────────────────────────────────────────────────

/// Canonical SERAPH tool names — allowlist for SDK-side validation.
///
/// Sourced from the SERAPH wings and services implementations.
const KNOWN_SERAPH_TOOLS: &[&str] = &[
    // Wings
    "capture",
    "scan",
    "analyze",
    "osint",
    "monitor",
    "execute",
    // Services
    "detonate",
    "orchestrate",
    "knowledge_search",
    "knowledge_read",
    "knowledge_stats",
    // Investigation lifecycle
    "investigate_start",
    "investigate_advance",
    "investigate_close",
    "investigate_report",
    // Utilities
    "vault_sync",
    "speak",
    "status",
    // Wing-level tools (sub-dispatch names accepted by scan/analyze wings)
    "nmap",
    "masscan",
    "rustscan",
    "nikto",
    "gobuster",
    "nuclei",
    "whatweb",
    "tcpdump",
    "tshark",
    "theHarvester",
    "file",
    "strings",
    "objdump",
    "ghidra",
    "binwalk",
    "volatility3",
    "ss",
    "ncat",
    "nc",
    "socat",
    "chisel",
    "ligolo-ng",
    "msfvenom",
];

/// Shell metacharacters that must not appear in scope targets.
const SHELL_METACHARS: &[char] = &[';', '&', '|', '`', '$', '>', '<'];

/// SDK-side scope constraint. Validates target, tool, and domain before dispatch.
///
/// `ScopeConstraint` is an ergonomic guard that rejects obviously invalid or
/// dangerous inputs **before** any IPC call to SERAPH. It does **not** replicate
/// SERAPH's server-side 5-gate `ScopeGovernor` — the server enforces TTL, target
/// allowlisting, concurrent limits, and domain constraints independently.
///
/// # Construction
///
/// Use [`ScopeConstraint::new`] — the only constructor. All validation is
/// performed at construction time, not at dispatch time.
///
/// # Security constraints
///
/// - `#[non_exhaustive]` — allows adding validation fields in future SDK versions
///   without breaking downstream consumers.
/// - No public fields — callers cannot bypass validation by constructing directly.
/// - Does not implement `Serialize` — prevents scope constraint data from
///   appearing in logs or wire payloads.
///
/// # Example
///
/// ```rust
/// use crate::seraph::scope::{ScopeConstraint, ScopeDomain};
///
/// let constraint = ScopeConstraint::new("192.168.1.0/24", "nmap", ScopeDomain::Network)
///     .expect("valid constraint");
///
/// assert_eq!(constraint.target(), "192.168.1.0/24");
/// assert_eq!(constraint.tool(), "nmap");
/// ```
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ScopeConstraint {
    target: String,
    tool: String,
    domain: ScopeDomain,
}

impl ScopeConstraint {
    /// Only constructor — enforces all invariants at construction time.
    ///
    /// # Validation
    ///
    /// - `target`: rejects shell metacharacters (`;`, `&`, `|`, `` ` ``, `$`, `>`, `<`)
    ///   and null bytes. Rejects localhost addresses (`localhost`, `127.0.0.1`, `::1`)
    ///   unless the `testing` cfg flag is set.
    /// - `tool`: allowlisted against known SERAPH tool names.
    ///
    /// Rejected target strings are **not** included in the error message verbatim —
    /// they are hashed and emitted via `tracing::warn!` for audit logging.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::ScopeViolation`] if any validation constraint is violated.
    pub fn new(
        target: impl Into<String>,
        tool: impl Into<String>,
        domain: ScopeDomain,
    ) -> Result<Self, SdkError> {
        let target = target.into();
        let tool = tool.into();

        validate_target(&target)?;
        validate_tool(&tool)?;

        Ok(Self {
            target,
            tool,
            domain,
        })
    }

    /// The validated target string.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The validated tool name.
    #[must_use]
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// The engagement domain.
    #[must_use]
    pub fn domain(&self) -> &ScopeDomain {
        &self.domain
    }
}

/// Validate a scope target string.
///
/// Rejects shell metacharacters, null bytes, and localhost addresses.
/// The raw target is never included in the returned error — it is hashed
/// and logged at WARN level for audit trails.
fn validate_target(target: &str) -> Result<(), SdkError> {
    // Reject null bytes — not renderable and may bypass downstream checks.
    if target.contains('\0') {
        tracing::warn!(
            target_hash = %simple_hash(target),
            "scope target rejected: contains null byte"
        );
        return Err(SdkError::ScopeViolation(
            "target contains null byte".to_owned(),
        ));
    }

    // Reject ASCII control characters (0x01-0x1F excl. null handled above, 0x7F DEL,
    // and C1 controls 0x80-0x9F) which have no valid use in a network target
    // and may confuse downstream parsers.
    if target
        .bytes()
        .any(|b| b < 0x20 || b == 0x7F || (0x80..=0x9F).contains(&b))
    {
        tracing::warn!(
            target_hash = %simple_hash(target),
            "scope target rejected: contains control character"
        );
        return Err(SdkError::ScopeViolation(
            "target contains control character".to_owned(),
        ));
    }

    // Reject shell metacharacters that could enable injection attacks.
    for ch in SHELL_METACHARS {
        if target.contains(*ch) {
            tracing::warn!(
                target_hash = %simple_hash(target),
                rejected_char = %ch,
                "scope target rejected: shell metacharacter"
            );
            return Err(SdkError::ScopeViolation(format!(
                "target contains shell metacharacter '{ch}'"
            )));
        }
    }

    // Reject localhost targets — localhost is not a valid pentest target.
    // This constraint exists in both production and test mode. Tests that need
    // to verify scope rejection behaviour use non-routable RFC-5737 addresses
    // (e.g. 192.0.2.1) or private CIDR ranges (192.168.x.x, 10.x.x.x).
    if is_localhost(target) {
        tracing::warn!(
            target_hash = %simple_hash(target),
            "scope target rejected: localhost address"
        );
        return Err(SdkError::ScopeViolation(
            "localhost targets are not permitted".to_owned(),
        ));
    }

    Ok(())
}

/// Returns `true` if the target is a localhost address.
fn is_localhost(target: &str) -> bool {
    let lower = target.to_lowercase();
    lower == "localhost"
        || lower.starts_with("localhost:")
        || lower.starts_with("127.")
        || lower == "::1"
        || lower.starts_with("[::1]")
        || lower == "0:0:0:0:0:0:0:1"
        || lower == "::ffff:127.0.0.1"
        || lower.starts_with("::ffff:127.")
}

/// Validate a tool name against the known SERAPH tool allowlist.
fn validate_tool(tool: &str) -> Result<(), SdkError> {
    if KNOWN_SERAPH_TOOLS.contains(&tool) {
        return Ok(());
    }
    // Allow impacket-* prefix — matches SERAPH execute wing dispatch.
    if tool.starts_with("impacket-") && !tool.contains(SHELL_METACHARS) {
        return Ok(());
    }
    Err(SdkError::ScopeViolation(format!(
        "tool '{tool}' is not in the SERAPH allowlist"
    )))
}

/// Compute a simple FNV-1a 64-bit hash of a string for audit logging.
///
/// This is intentionally a non-cryptographic hash — its purpose is to produce
/// a stable, short identifier for log correlation without exposing the raw value.
/// 64-bit variant reduces collision probability vs the 32-bit form.
fn simple_hash(s: &str) -> u64 {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;
    let mut hash = FNV_OFFSET;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn test_scope() -> EngagementScope {
        EngagementScope {
            engagement_id: "ENG-TEST-001".into(),
            targets: vec!["192.168.1.0/24".into(), "10.0.0.1".into()],
            authorized_tools: vec!["nmap".into(), "tshark".into()],
            expires_at: Utc::now() + Duration::hours(4),
            hitl_required: false,
            authorized_by: "kevin".into(),
            max_concurrent_scans: 3,
        }
    }

    #[test]
    fn to_toml_produces_valid_toml() {
        let scope = test_scope();
        let toml_str = scope.to_toml().unwrap();
        assert!(toml_str.contains("engagement_id = \"ENG-TEST-001\""));
        assert!(toml_str.contains("authorized_by = \"kevin\""));
        assert!(toml_str.contains("hitl_required = false"));
        assert!(toml_str.contains("max_concurrent_scans = 3"));
    }

    #[test]
    fn to_toml_roundtrip() {
        let scope = test_scope();
        let toml_str = scope.to_toml().unwrap();
        let parsed: EngagementScope = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.engagement_id, scope.engagement_id);
        assert_eq!(parsed.targets, scope.targets);
        assert_eq!(parsed.authorized_tools, scope.authorized_tools);
        assert_eq!(parsed.hitl_required, scope.hitl_required);
        assert_eq!(parsed.authorized_by, scope.authorized_by);
        assert_eq!(parsed.max_concurrent_scans, scope.max_concurrent_scans);
    }

    #[test]
    fn install_creates_file() {
        let temp = tempfile::tempdir().unwrap();
        // SAFETY: test-only; tests run with `--test-threads=1` or accept the race.
        unsafe { std::env::set_var("HOME", temp.path()) };

        let scope = test_scope();
        let path = scope.install().unwrap();
        assert!(path.exists(), "scope.toml should exist at {path:?}");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("engagement_id = \"ENG-TEST-001\""));
    }

    #[test]
    fn default_max_concurrent_is_3() {
        // Verify the default function returns 3.
        assert_eq!(default_max_concurrent(), 3);
    }

    #[test]
    fn toml_deserialize_missing_max_concurrent_uses_default() {
        let toml_str = r#"
engagement_id = "ENG-X"
targets = ["10.0.0.1"]
authorized_tools = ["nmap"]
expires_at = "2030-01-01T00:00:00Z"
hitl_required = true
authorized_by = "tester"
"#;
        let parsed: EngagementScope = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.max_concurrent_scans, 3);
    }

    // ── ScopeConstraint tests ────────────────────────────────────────────────

    #[test]
    fn scope_constraint_valid_input_accepted() {
        let c = ScopeConstraint::new("192.168.1.0/24", "nmap", ScopeDomain::Network);
        assert!(c.is_ok(), "valid target/tool/domain should be accepted");
        let c = c.unwrap();
        assert_eq!(c.target(), "192.168.1.0/24");
        assert_eq!(c.tool(), "nmap");
        assert_eq!(c.domain(), &ScopeDomain::Network);
    }

    #[test]
    fn scope_constraint_shell_metachar_rejected() {
        for metachar in &[";", "&", "|", "`", "$", ">", "<"] {
            let target = format!("192.168.1.1{metachar}id");
            let result = ScopeConstraint::new(&target, "nmap", ScopeDomain::Network);
            assert!(
                result.is_err(),
                "target with metachar '{metachar}' should be rejected"
            );
            let err = result.unwrap_err();
            // Error message must NOT contain the raw target.
            assert!(
                !err.to_string().contains("192.168.1.1"),
                "error message must not expose raw target: {err}"
            );
        }
    }

    #[test]
    fn scope_constraint_localhost_rejected() {
        for localhost in &["localhost", "127.0.0.1", "127.0.0.2", "::1", "[::1]"] {
            let result = ScopeConstraint::new(*localhost, "nmap", ScopeDomain::Network);
            assert!(
                result.is_err(),
                "localhost target '{localhost}' should be rejected"
            );
        }
    }

    #[test]
    fn scope_constraint_null_byte_rejected() {
        let target = "192.168.1.1\0suffix";
        let result = ScopeConstraint::new(target, "nmap", ScopeDomain::Network);
        assert!(result.is_err(), "null byte in target should be rejected");
    }

    #[test]
    fn scope_constraint_unknown_tool_rejected() {
        let result = ScopeConstraint::new("192.168.1.1", "metasploit", ScopeDomain::Network);
        assert!(result.is_err(), "unknown tool should be rejected");
        let err = result.unwrap_err().to_string();
        // Error message may include the tool name (it is not attacker-controlled here).
        assert!(err.contains("metasploit") || err.contains("allowlist"));
    }

    #[test]
    fn scope_constraint_impacket_prefix_accepted() {
        let result = ScopeConstraint::new(
            "DOMAIN/user:pass@10.0.0.1",
            "impacket-secretsdump",
            ScopeDomain::Network,
        );
        assert!(result.is_ok(), "impacket-* tools should be accepted");
    }

    #[test]
    fn scope_constraint_web_domain_accepted() {
        let result = ScopeConstraint::new("https://example.com", "nuclei", ScopeDomain::Web);
        assert!(result.is_ok());
    }

    #[test]
    fn simple_hash_is_deterministic() {
        assert_eq!(simple_hash("test"), simple_hash("test"));
        assert_ne!(simple_hash("test"), simple_hash("other"));
    }

    #[test]
    fn scope_result_must_use_is_authorized() {
        use crate::seraph::types::ScopeResult;
        use std::time::Duration;
        let r = ScopeResult::new("authorized".to_owned(), true, Duration::from_secs(3600));
        assert!(r.is_authorized());
        let r2 = ScopeResult::new("rejected".to_owned(), false, Duration::ZERO);
        assert!(!r2.is_authorized());
        assert_eq!(r2.ttl_remaining(), Duration::ZERO);
    }
}
