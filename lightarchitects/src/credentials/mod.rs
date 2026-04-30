//! External CLI credential detection.
//!
//! Detects the presence of OAuth / API-key credentials for third-party
//! CLIs (Anthropic Claude, OpenAI Codex, Google Gemini) so host apps can
//! skip redundant auth prompts when the target CLI is already logged in.
//!
//! # Design
//!
//! - **Plugin registry** ([`Registry`]): one trait
//!   ([`CliCredentialProvider`]), one registry. Adding a CLI = new
//!   submodule + a single registry hook.
//! - **Opaque public surface** ([`Locator`]): Absent / Keychain / File /
//!   Env. No canonical service or file strings exposed by default.
//! - **Feature-gated detail** (`credentials-detailed-locator`): when the
//!   UI needs render-quality strings.
//! - **30-second TTL cache**: matches the target CLIs' own cache
//!   semantics and bounds keychain subprocess spawns.
//!
//! # Security
//!
//! Probes for *presence* only. Credential content never enters the
//! process: keychain subprocess I/O is redirected to `/dev/null`; file
//! probes are existence-only.
//!
//! **Non-logging policy**: canonical service, file, and env names are
//! scoped to the provider module that needs them. Custom `Debug` impls
//! redact canonical strings in both [`Detection`] and [`DetailedLocator`].
//! When adding log sites that touch this module, use [`ProviderId`] in
//! structured fields — never the canonical strings.
//!
//! # Usage
//!
//! ```no_run
//! # #[cfg(feature = "credentials")]
//! # async fn demo() {
//! use lightarchitects::credentials::{default_registry, Locator};
//!
//! let reg = default_registry();
//! for detection in reg.probe_all().await {
//!     match detection.locator {
//!         Locator::Absent => { /* user not logged in */ }
//!         Locator::Keychain | Locator::File | Locator::Env => {
//!             /* skip credential prompt */
//!         }
//!     }
//! }
//! # }
//! ```

mod cache;
mod platform;
mod providers;
mod registry;
mod types;

#[cfg(test)]
mod tests;

pub use registry::{CliCredentialProvider, Registry, default_registry};
pub use types::{Detection, Locator, ProbeError, ProviderId};

#[cfg(feature = "credentials-detailed-locator")]
pub use types::DetailedLocator;

#[cfg(feature = "providers-anthropic")]
pub use providers::anthropic_cli::ID as ANTHROPIC_CLI;
#[cfg(feature = "providers-google")]
pub use providers::google_cli::ID as GOOGLE_CLI;
#[cfg(feature = "providers-openai")]
pub use providers::openai_cli::ID as OPENAI_CLI;
