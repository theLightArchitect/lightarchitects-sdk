//! Public types for the credentials module.
//!
//! - [`ProviderId`]: opaque 16-byte identifier. Rendered only as hex in
//!   `Debug`/`Display` — never as canonical service or file names.
//! - [`Detection`]: probe result (abstract).
//! - [`Locator`]: abstract presence category (default public surface).
//! - [`DetailedLocator`]: canonical strings (feature-gated).
//! - [`ProbeError`]: probe failure variants.
//!
//! The abstract `Locator` is the default public surface. Consumers learn
//! *that* auth exists, not *what* service/account/path holds it. Enable the
//! `credentials-detailed-locator` feature to access the richer variant.

use std::fmt;

#[cfg(feature = "credentials-detailed-locator")]
use std::path::PathBuf;

/// Opaque 16-byte identifier for a credential provider.
///
/// Stable across compiles (providers hardcode their ID constant). Suitable
/// for keys in maps, log fields, and UI state without exposing the
/// underlying canonical service/file names.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProviderId(pub(crate) [u8; 16]);

impl fmt::Debug for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProviderId(")?;
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0[..4] {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// Result of probing a single credential provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    /// Opaque provider identifier.
    pub provider_id: ProviderId,
    /// Whether any credential source was found.
    pub available: bool,
    /// Abstract source category.
    pub locator: Locator,
}

/// Abstract category of where credentials were found.
///
/// Default public surface. Canonical strings (service names, file paths,
/// env var names) are **not** exposed here; use [`DetailedLocator`] behind
/// the `credentials-detailed-locator` feature when the UI needs them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Locator {
    /// No credentials found.
    Absent,
    /// Found in an OS keychain / keyring entry.
    Keychain,
    /// Found in an on-disk credentials file.
    File,
    /// Found via an environment variable.
    Env,
}

/// Richer locator variant with canonical strings for UI rendering.
///
/// Only compiled when `credentials-detailed-locator` is enabled. Consumers
/// must enforce their own logging policy — the SDK does not log these
/// strings, and the [`fmt::Debug`] impl redacts them.
#[cfg(feature = "credentials-detailed-locator")]
#[derive(Clone, PartialEq, Eq)]
pub enum DetailedLocator {
    /// No credentials found.
    Absent,
    /// macOS Keychain / Linux libsecret entry.
    Keychain {
        /// Canonical service name as known to the target CLI.
        service: String,
        /// Account/username associated with the entry.
        account: String,
    },
    /// On-disk credentials file (existence-only probe; content never read).
    File {
        /// Absolute path to the credentials file.
        path: PathBuf,
    },
    /// Environment variable holding a credential.
    Env {
        /// Name of the environment variable.
        var: &'static str,
    },
}

#[cfg(feature = "credentials-detailed-locator")]
impl fmt::Debug for DetailedLocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: redact canonical strings in Debug output. The SDK's
        // non-logging policy relies on this impl; do not change it to print
        // service/account/path/var.
        match self {
            Self::Absent => write!(f, "DetailedLocator::Absent"),
            Self::Keychain { .. } => write!(f, "DetailedLocator::Keychain(<redacted>)"),
            Self::File { .. } => write!(f, "DetailedLocator::File(<redacted>)"),
            Self::Env { .. } => write!(f, "DetailedLocator::Env(<redacted>)"),
        }
    }
}

/// Errors that can arise during a provider probe.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// Platform I/O failure (e.g. subprocess spawn).
    #[error("platform I/O error (provider {provider_id})")]
    Io {
        /// Opaque provider identifier.
        provider_id: ProviderId,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// Keychain subprocess invocation itself failed (not a "not found" result).
    #[error("keychain subprocess failed (provider {provider_id})")]
    Keychain {
        /// Opaque provider identifier.
        provider_id: ProviderId,
    },
}
