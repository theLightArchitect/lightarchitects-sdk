//! Project identity and ingestion — `projects/` module.
//!
//! Phase 2 exposes read-only types + slug validation.
//! Phase 3 adds the write path (`init.rs`, `audit.rs`).

pub mod audit;
pub mod init;
pub mod types;

pub use types::{AgentRole, ProjectAgents, ProjectGit, ProjectKind, ProjectMeta, Slug, SlugError};

pub use lightarchitects_arch::security::path::{
    PathError as ArchPathError, canonicalize_and_check,
};
