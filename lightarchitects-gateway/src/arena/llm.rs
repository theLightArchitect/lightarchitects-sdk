//! Re-export of the shared LLM client for backward compatibility within `arena`.
//!
//! The canonical location is [`crate::llm`]. New code should use that path directly.

pub use crate::llm::*;
