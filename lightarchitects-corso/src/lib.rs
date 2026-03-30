//! Typed client for the CORSO MCP server.
//!
//! CORSO exposes a single MCP tool — `corsoTools` — with 26 actions covering
//! filesystem access, code intelligence, AI analysis (SNIFF/GUARD/FETCH/CHASE),
//! code generation, and operational management.
//!
//! Responses from CORSO are wrapped in the MCP `ToolCallResult` content-block
//! format. This crate transparently unwraps that envelope before returning
//! typed values to callers.
//!
//! # Quick start
//!
//! ```no_run
//! use lightarchitects_corso::CorsoClient;
//!
//! # async fn example() -> Result<(), lightarchitects_core::SdkError> {
//! let client = CorsoClient::builder().build().await?;
//!
//! // Structured response: read a source file
//! let file = client.read_file("/path/to/lib.rs", None).await?;
//! println!("Read {} bytes from {}", file.content.len(), file.path);
//!
//! // AI analysis response: GUARD security audit
//! let audit = client.guard("/path/to/src/").await?;
//! println!("{}", audit.output);
//!
//! // Code search
//! let hits = client.search_code("fn call_tool", None).await?;
//! for h in hits {
//!     println!("{}:{} — {}", h.file, h.line, h.content);
//! }
//! # Ok(()) }
//! ```

/// Canonical CORSO action enum — code quality, security, ops, verification.
pub mod actions;
mod client;
mod content;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use actions::CorsoAction;
pub use client::{CorsoClient, CorsoClientBuilder};
pub use types::{
    ActionOutput, CodeSearchHit, ContainerOp, DirEntry, DirectoryListing, FileContent, FileOutline,
    FileWritten, OutlineEntry, ReferenceLocation, ReferenceResult, SecretOp, SymbolLocation,
    SymbolSearchResult,
};
