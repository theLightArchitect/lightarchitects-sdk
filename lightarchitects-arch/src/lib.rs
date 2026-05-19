//! # lightarchitects-arch
//!
//! Architecture intelligence for the Light Architects platform.
//! Extracts, verifies, and emits C4+ diagrams from Rust, TypeScript, and Python codebases.
//!
//! ## Abstraction levels
//!
//! Nodes are modelled at seven levels (L0–L6) following the C4 model extended with
//! Dependency and Runtime layers.  See [`model::ArchLevel`] for the full hierarchy.
//!
//! ## Security
//!
//! All external-input paths route through the [`security`] module before touching the OS:
//!
//! - [`security::path`] — path canonicalization with per-segment symlink guard (S-3/H1)
//! - [`security::cmd_exec`] — subprocess execution with binary + flag allowlists (B1/S-1)
//! - [`security::encode`] — HTML output encoding with scheme rejection (H2/S-4)
//!
//! ## Pipeline
//!
//! ```text
//! Source files
//!     │  tree-sitter parse
//!     ▼
//! ExtractedFacts  ──▶  ArchModel  ──▶  Findings
//!                                          │
//!                                          ▼
//!                                     Mermaid / D2 / HTML
//! ```

#![deny(missing_docs)]

pub mod emitter;
pub mod extractor;
pub mod model;
pub mod narrative;
pub mod security;

pub use model::{
    ArchFinding, ArchLevel, ArchModel, ArchNode, ArchRelation, ExtractedFacts, FindingClass,
    Language, RelationKind, Severity,
};
