//! Container spawn policy — `SpawnPolicy` trait, isolation modes, network policy,
//! resource caps, and lock-free policy store via `ArcSwap`.
//!
//! # Quick start
//!
//! ```rust
//! use lightarchitects::container_spawn::policy::{
//!     ContainerPolicy, IsoMode, NetworkPolicy, PolicyStore, SpawnPolicy,
//! };
//!
//! // Build with defaults (Standard tier, Bridge network).
//! let store = PolicyStore::new(ContainerPolicy::default())?;
//!
//! // Lock-free read — cheap Arc clone, no locking.
//! let policy = store.effective_policy();
//! let args = policy.build_docker_args()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Phase-2 features
//!
//! [`policy::NetworkPolicy::Balanced`] and
//! [`policy::CredentialStrategy::Proxy`] are declared for API stability but
//! return [`policy::SpawnError::NotYetImplemented`] until Phase 2 ships.
//! See [`disclosure::PHASE_2_DISCLOSURE`] for the human-readable message
//! surfaced to callers.

pub mod disclosure;
pub mod handle;
pub mod policy;
pub mod seccomp;
pub mod system;

#[cfg(test)]
mod tests;

// Re-export the most commonly needed surface so callers can write
// `use lightarchitects::container_spawn::*;` without layering module paths.
pub use handle::{ContainerHandle, HardeningLevel, UsernsState};
pub use policy::{
    AgentTier, ContainerPolicy, ContainerResources, CredentialStrategy, IsoMode, NetworkPolicy,
    PolicyStore, SpawnError, SpawnPolicy,
};
pub use seccomp::SECCOMP_PROFILE_JSON;
