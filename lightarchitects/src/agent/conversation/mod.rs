//! L2 conversation session substrate — structured turn management, memory,
//! transport, and lifecycle hooks.
//!
//! Enabled by the `loops-core` feature. This module is the SDK-promoted
//! counterpart of the gateway `agent_stream` package; see
//! `lightarchitects-gateway/src/agent_stream/` for the DEPRECATED shim.

pub mod event;
#[cfg(feature = "agent-cli")]
pub mod helix_memory;
pub mod memory;
pub mod session;
pub mod transport;

pub use event::{ConversationEvent, TerminationReason};
#[cfg(feature = "agent-cli")]
pub use helix_memory::HelixSessionMemory;
pub use memory::{ConversationMemory, InMemoryConversationMemory, MessageRole, Turn};
pub use session::{ControlMessage, ConversationSession, SessionConfig, SessionError, SessionState};
pub use transport::{NdjsonTransport, SseTransport, Transport, TtyTransport};
