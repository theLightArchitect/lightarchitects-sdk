//! Connection-time authentication provider.
//!
//! Defines the [`AuthProvider`] trait and the type-erased [`AuthChecker`]
//! wrapper used by [`lightarchitects::core::StdioTransport::connect`].
//!
//! ## Design
//!
//! ```text
//! AuthProvider (trait, not object-safe — RPITIT)
//!     └── AuthChecker::from_provider(impl AuthProvider)
//!              └── type-erased Arc<dyn Fn() → BoxFuture>
//!                       └── stored in SiblingClientBuilder
//!                                └── passed to StdioTransport::connect()
//! ```
//!
//! Sibling clients depend only on `lightarchitects-core` — they accept
//! `impl AuthProvider` without knowing about the concrete `AuthGuard` type
//! from `lightarchitects-auth`. The caller wires them together.
//!
//! ## Auth flow
//!
//! 1. Caller creates `AuthGuard` (from `lightarchitects-auth`).
//! 2. Caller passes it to the sibling client builder via `.auth(guard)`.
//! 3. `build().await` calls `AuthChecker::check()` before spawning the binary.
//! 4. `AuthStatus::Valid` → binary spawns normally.
//! 5. `AuthStatus::Degraded` → binary spawns with a `WARN` log.
//! 6. `Err(SdkError::Auth)` → build fails immediately, no process opened.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::core::error::SdkError;

// ── AuthStatus ────────────────────────────────────────────────────────────────

/// Outcome of a connection-time auth check.
#[derive(Debug, Clone)]
pub enum AuthStatus {
    /// Fully authenticated — proceed normally.
    Valid,
    /// Auth degraded (cache expired, validation endpoint unreachable).
    ///
    /// The binary will still be spawned. A `WARN`-level log is emitted.
    /// Callers requiring strict auth should treat `Degraded` as an error.
    Degraded {
        /// Human-readable reason, e.g. "grace period (2 resets remaining)".
        message: String,
    },
}

// ── AuthProvider ──────────────────────────────────────────────────────────────

/// Provides connection-time authentication for sibling MCP clients.
///
/// Implement this trait to gate [`lightarchitects::core::StdioTransport::connect`] behind an
/// auth check. `lightarchitects-auth` provides [`AuthGuard`] as the production
/// implementation.
///
/// # Object safety
///
/// This trait is **not object-safe** (RPITIT return type). Store as an
/// [`AuthChecker`] via [`AuthChecker::from_provider`] when you need to hold
/// it in a struct or pass it to functions expecting an erased type.
///
/// # Example
///
/// ```no_run
/// # use lightarchitects::core::auth::{AuthProvider, AuthStatus};
/// # use lightarchitects::core::SdkError;
/// struct AlwaysValid;
///
/// impl AuthProvider for AlwaysValid {
///     async fn check_connect(&self) -> Result<AuthStatus, SdkError> {
///         Ok(AuthStatus::Valid)
///     }
/// }
/// ```
pub trait AuthProvider: Send + Sync + 'static {
    /// Check auth before the MCP subprocess is spawned.
    ///
    /// - `Ok(AuthStatus::Valid)` — spawn the binary normally.
    /// - `Ok(AuthStatus::Degraded { .. })` — spawn with a warning.
    /// - `Err(SdkError::Auth(_))` — hard failure, do **not** spawn.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Auth`] when authentication is required and fails
    /// (no key found, key revoked, or grace period exhausted).
    fn check_connect(&self) -> impl Future<Output = Result<AuthStatus, SdkError>> + Send + '_;
}

// ── AuthChecker ───────────────────────────────────────────────────────────────

/// Internal type for the type-erased auth check function.
type CheckFn =
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<AuthStatus, SdkError>> + Send>> + Send + Sync;

/// Type-erased, [`Clone`]-able wrapper around any [`AuthProvider`].
///
/// Produced by [`AuthChecker::from_provider`]. Stored in sibling client
/// builders and passed to [`lightarchitects::core::StdioTransport::connect`]. No generic
/// parameter is required — the underlying provider is fully erased.
#[derive(Clone)]
pub struct AuthChecker(Arc<CheckFn>);

impl AuthChecker {
    /// Wrap any [`AuthProvider`] implementation into a cloneable checker.
    ///
    /// The provider is captured in an `Arc` so the `AuthChecker` can be
    /// cloned cheaply without duplicating the provider state.
    pub fn from_provider<A: AuthProvider>(provider: A) -> Self {
        let provider = Arc::new(provider);
        Self(Arc::new(move || {
            let p = provider.clone();
            Box::pin(async move { p.check_connect().await })
        }))
    }

    /// Run the auth check.
    ///
    /// # Errors
    ///
    /// Propagates [`SdkError::Auth`] from the underlying provider on hard
    /// auth failure.
    pub async fn check(&self) -> Result<AuthStatus, SdkError> {
        (self.0)().await
    }
}

impl std::fmt::Debug for AuthChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthChecker").finish_non_exhaustive()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysValid;

    impl AuthProvider for AlwaysValid {
        async fn check_connect(&self) -> Result<AuthStatus, SdkError> {
            Ok(AuthStatus::Valid)
        }
    }

    struct AlwaysDegraded;

    impl AuthProvider for AlwaysDegraded {
        async fn check_connect(&self) -> Result<AuthStatus, SdkError> {
            Ok(AuthStatus::Degraded {
                message: "test degradation".to_owned(),
            })
        }
    }

    struct AlwaysFail;

    impl AuthProvider for AlwaysFail {
        async fn check_connect(&self) -> Result<AuthStatus, SdkError> {
            Err(SdkError::Auth("no key".to_owned()))
        }
    }

    #[tokio::test]
    async fn valid_checker_returns_ok() {
        let checker = AuthChecker::from_provider(AlwaysValid);
        let result = checker.check().await;
        assert!(matches!(result, Ok(AuthStatus::Valid)));
    }

    #[tokio::test]
    async fn degraded_checker_returns_degraded() {
        let checker = AuthChecker::from_provider(AlwaysDegraded);
        let result = checker.check().await;
        assert!(matches!(result, Ok(AuthStatus::Degraded { .. })));
        if let Ok(AuthStatus::Degraded { message }) = result {
            assert_eq!(message, "test degradation");
        }
    }

    #[tokio::test]
    async fn failing_checker_returns_auth_error() {
        let checker = AuthChecker::from_provider(AlwaysFail);
        let result = checker.check().await;
        assert!(matches!(result, Err(SdkError::Auth(_))));
    }

    #[tokio::test]
    async fn checker_is_clone() {
        let checker = AuthChecker::from_provider(AlwaysValid);
        let checker2 = checker.clone();
        let r1 = checker.check().await;
        let r2 = checker2.check().await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }
}
