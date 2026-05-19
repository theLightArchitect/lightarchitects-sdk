//! Diagram and documentation emitters for [`ArchModel`].
//!
//! Emitters are pure functions: `ArchModel + NarrativeSeed → text artifact`.
//! All text from the model is treated as untrusted and encoded at the HTML boundary.

pub mod checklist;
pub mod d2;
pub mod html;
pub mod likec4;
pub mod markdown;
pub mod mermaid;

use crate::{model::ArchModel, narrative::NarrativeSeed};

/// Errors returned by emitters.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    /// An I/O error occurred while writing output.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A model field contained content that could not be safely encoded.
    #[error("encoding error in field '{field}': {reason}")]
    Encode {
        /// The model field that failed encoding.
        field: String,
        /// Why encoding failed.
        reason: String,
    },
    /// An external tool (e.g. `diff`, `grep`) returned a non-zero exit code.
    #[error("command execution error: {0}")]
    Cmd(String),
}

/// Emits a Likec4 DSL representation of `model`.
///
/// # Errors
///
/// Returns [`EmitError`] if the model contains unencodable field content.
pub fn emit_likec4(model: &ArchModel) -> Result<String, EmitError> {
    likec4::emit(model)
}

/// Emits a Mermaid block diagram with `securityLevel: 'strict'`.
///
/// # Errors
///
/// Returns [`EmitError`] if the model is empty (no nodes).
pub fn emit_mermaid(model: &ArchModel) -> Result<String, EmitError> {
    mermaid::emit(model)
}

/// Emits a D2 diagram.
///
/// # Errors
///
/// Returns [`EmitError`] on encoding failures.
pub fn emit_d2(model: &ArchModel) -> Result<String, EmitError> {
    d2::emit(model)
}

/// Emits a Markdown document using pulldown-cmark `safe` mode (raw HTML stripped).
///
/// # Errors
///
/// Returns [`EmitError`] on encoding failures.
pub fn emit_markdown(model: &ArchModel, seed: Option<&NarrativeSeed>) -> Result<String, EmitError> {
    markdown::emit(model, seed)
}

/// Emits a full HTML document.
///
/// When `seed` is `Some`, narrative sections from the seed are merged into the output.
/// All model-derived text is HTML-encoded before insertion.
///
/// # Errors
///
/// Returns [`EmitError`] on encoding failures.
pub fn emit_html(
    model: &ArchModel,
    seed: Option<&NarrativeSeed>,
    skeleton_only: bool,
) -> Result<String, EmitError> {
    html::emit(model, seed, skeleton_only)
}
