//! Shared LLM stream parser module (TS-3 §21.3).
//!
//! Four sub-modules cover the wire formats used by the supported providers:
//!
//! | Module | Wire format | Used by |
//! |--------|-------------|---------|
//! | [`framing`] | Raw bytes → complete lines | All providers |
//! | [`sse`] | Anthropic SSE `data: {json}` | `AnthropicHttpProvider`, `OllamaCliProvider` |
//! | [`stream_json`] | Claude CLI NDJSON | `ClaudeCliProvider` |
//! | [`openai_sse`] | OpenAI `/chat/completions` SSE | `OpenAICompatProvider` |
//!
//! All four modules emit [`ProviderEvent`] — call sites are provider-agnostic.
//!
//! [`ProviderEvent`]: crate::agent::ProviderEvent

pub mod framing;
pub mod openai_sse;
pub mod sse;
pub mod stream_json;

pub use framing::{FramingError, LineSplitter, MAX_LINE_BYTES};
pub use openai_sse::{OpenAiSseParseError, OpenAiStreamState, parse_openai_sse_line};
pub use sse::{SseParseError, parse_sse_line};
pub use stream_json::{NdjsonParseError, parse_ndjson_line};
