//! Permission matrix and cost gate types for `AgentRunner` spawns.
//!
//! These types are applied at spawn time by [`super::ClaudeCliProvider`] when
//! used in a lightsquad worker context.  The defaults are **fail-closed**:
//! no tools, no bash, no file writes, no network.
//!
//! # Security model
//!
//! Per ADR-010 and the platform Permission Matrix (Canon §Security Guardrails §6):
//! - Callers MUST supply an explicit [`PermissionMatrix`] via
//!   [`super::ClaudeCliProvider::with_permission_matrix`] when
//!   `require_permission_matrix` is set to `true`.
//! - When `require_permission_matrix` is `true` and no matrix is supplied, the
//!   spawn is rejected with [`super::ProviderError::MissingPermissionMatrix`].
//! - The matrix is applied by injecting `--disallowed-tools` flags into the
//!   `claude -p` invocation at subprocess build time.

/// Fail-closed permission matrix for `AgentRunner` spawns.
///
/// All fields default to the most restrictive option.  Grant only what the
/// specific worker tier requires.
///
/// # Example
///
/// ```rust
/// use lightarchitects::agent::permissions::PermissionMatrix;
///
/// let matrix = PermissionMatrix {
///     allowed_tools: vec!["Read".to_owned(), "Edit".to_owned()],
///     allow_bash: false,
///     allow_file_write: false,
///     allow_network: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct PermissionMatrix {
    /// Explicit allowlist of Claude tool names (e.g. `"Read"`, `"Edit"`, `"Bash"`).
    ///
    /// This list is forwarded to the `--tools` flag on the `claude -p` subprocess.
    /// An empty vec means **no tools permitted**.
    pub allowed_tools: Vec<String>,

    /// Permit the `Bash` tool inside the subprocess.
    ///
    /// Even when `true`, the `allowed_tools` list must include `"Bash"` for the
    /// subprocess to actually call it — this flag is an additional gate checked
    /// before building the command.
    pub allow_bash: bool,

    /// Permit file-write operations (`Write`, `Edit`, `MultiEdit` tools).
    pub allow_file_write: bool,

    /// Permit network access from within the subprocess (`WebFetch`, etc.).
    pub allow_network: bool,
}

impl Default for PermissionMatrix {
    /// Returns the most restrictive matrix: nothing allowed.
    fn default() -> Self {
        Self {
            allowed_tools: vec![],
            allow_bash: false,
            allow_file_write: false,
            allow_network: false,
        }
    }
}

impl PermissionMatrix {
    /// Derive the final tool allowlist, reconciled against the boolean gates.
    ///
    /// Tools that require a capability flag (e.g. `"Bash"` requires
    /// `allow_bash`) are silently removed when the flag is `false`.  This
    /// ensures the CLI flags and the struct fields are always consistent.
    pub fn effective_tools(&self) -> Vec<String> {
        self.allowed_tools
            .iter()
            .filter(|t| {
                let name = t.to_lowercase();
                match name.as_str() {
                    "bash" => self.allow_bash,
                    "write" | "multiedit" => self.allow_file_write,
                    "webfetch" | "webcrawl" => self.allow_network,
                    _ => true,
                }
            })
            .cloned()
            .collect()
    }
}

/// Cost gate: reject a spawn if the pre-flight cost estimate exceeds `max_usd`.
///
/// The estimate is computed by [`super::ClaudeCliProvider::estimate_cost`] before
/// the subprocess is launched.  If the estimate exceeds `max_usd`, the provider
/// returns [`super::ProviderError::BudgetExceeded`] without spawning any process.
#[derive(Debug, Clone)]
pub struct CostGate {
    /// Maximum permitted estimated cost in USD.  Spawns with a higher estimate
    /// are rejected before any subprocess is launched.
    pub max_usd: f64,
}

impl Default for CostGate {
    /// Default gate: $1.00 per spawn.
    fn default() -> Self {
        Self { max_usd: 1.0 }
    }
}
