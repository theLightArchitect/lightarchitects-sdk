//! Core tool implementations for the `lightarchitects` gateway.
//!
//! Each sub-module exposes a single `run` function with the signature:
//! ```text
//! async fn run(params: serde_json::Value) -> Result<serde_json::Value, GatewayError>
//! ```
//!
//! The returned `Value` is the MCP tool-result payload:
//! `{"content": [{"type": "text", "text": "<result>"}]}`.

pub mod arena;
pub mod ask_user;
pub mod bash;
pub mod canon_check;
pub mod canon_evaluate;
pub mod discover;
pub mod edit;
pub mod glob;
pub mod import_adapter;
pub mod initialize;
pub mod meta;
pub mod ollama;
pub mod orchestrate;
pub mod read;
pub mod search;
pub mod write;

use serde_json::{Value, json};

/// Wrap a text string in the standard MCP tool-result envelope.
#[must_use]
pub fn text_result(text: impl Into<String>) -> Value {
    json!({
        "content": [{"type": "text", "text": text.into()}]
    })
}
