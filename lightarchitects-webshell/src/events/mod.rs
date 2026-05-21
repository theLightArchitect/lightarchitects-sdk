//! Server-side event subsystem for the webshell.
//!
//! This module collects events from three sources:
//!
//! - [`ayin_client`] — subscribes to AYIN SSE at `localhost:3742/events`
//!   and forwards every [`TraceSpan`] as a [`WebEvent::AyinSpan`].
//! - [`helix_watcher`] (Phase 4) — watches the vault filesystem and
//!   emits [`WebEvent::HelixEntry`] on new or modified Markdown files.
//! - [`control`] — HTTP POST endpoint that accepts [`ControlCommand`]s
//!   from external processes (e.g. Claude Code) and broadcasts them as
//!   [`WebEvent::Control`] for browser UI mutation.
//!
//! A [`tokio::sync::broadcast`] channel with [`ayin_client::EVENT_CHANNEL_BUF`]
//! capacity is owned by [`crate::server::AppState`]. The Phase-5 SSE handler
//! subscribes to that channel and fans each [`WebEvent`] out to every connected
//! browser as a `data:` payload.
//!
//! [`TraceSpan`]: types::TraceSpanSummary

pub mod ayin_client;
pub mod builds_handler;
pub mod control;
pub mod decisions;
pub mod envelope;
pub mod global_events;
pub mod helix_watcher;
pub mod lightsquad_bridge;
pub mod notify;
pub mod soul; // task #51 split target — see events/soul/mod.rs
pub mod soul_routes; // shim during partial-split window; re-exports from soul::*
pub mod sse_handler;
pub mod strand;
pub mod supervisor_handler;
pub mod topic_filter;
pub mod types;

pub use ayin_client::{AyinClient, EVENT_CHANNEL_BUF};
pub use builds_handler::builds_handler;
pub use control::control_handler;
pub use envelope::{Severity, WebEventV2};
pub use global_events::GlobalEventStore;
pub use helix_watcher::HelixWatcher;
pub use strand::{parse_strand_activations, window_aggregate};
pub use supervisor_handler::SupervisorEntry;
pub use topic_filter::TopicFilter;
pub use types::{
    BuildEventKind, BuildUpdateEvent, ControlCommand, CopilotActivityEvent, GlobalEventEntry,
    StrandActivationEvent, WebEvent,
};
