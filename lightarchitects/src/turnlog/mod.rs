//! # lightarchitects-turnlog
//!
//! Tier-1 ephemeral transactional log for conversational agents.
//!
//! `turnlog` is the unified substrate behind the Light Architects persistence
//! stack. Every turn, tool call, span, compaction cycle, reflection, and
//! lifecycle marker is written as an [`ayin::TraceSpan`] wrapped in a
//! [`TurnEntry`], forming a single append-only NDJSON session file that is
//! HMAC-chained for tamper detection.
//!
//! Three projections consume the log:
//!
//! 1. **SOUL helix** — `reflection` and `session_paused` spans promoted to Tier-2
//! 2. **AYIN waterfall** — entries ARE spans; no projection needed (see [`projection::waterfall`])
//! 3. **Training data** — `CanonicalTurn` rows joined from turn/tool/reflection entries
//!
//! # Design contract
//!
//! See the [`entry`] module for the on-disk schema. See [`chain`] for the HMAC
//! chain trust model. See [`store`] for the filesystem layout.
//!
//! # Quick start
//!
//! ```no_run
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use ayin::span::{Actor, TraceContext, TraceOutcome};
//! use lightarchitects::turnlog::{TurnLogWriter, StoreLayout, EndReason};
//! use secrecy::SecretSlice;
//! use std::path::PathBuf;
//!
//! let layout = StoreLayout::new(PathBuf::from("/tmp/turnlog-demo"));
//! let pepper = SecretSlice::from(vec![0_u8; 32]);
//! let writer = TurnLogWriter::open(
//!     &layout,
//!     "session-abc".to_owned(),
//!     PathBuf::from("/project"),
//!     "claude-opus-4-6".to_owned(),
//!     "anthropic".to_owned(),
//!     None,
//!     &pepper,
//! ).await?;
//!
//! // Callers construct ayin::TraceSpan values and append them directly.
//! let span = TraceContext::new(Actor::claude(), "turn.user")
//!     .session_id("session-abc")
//!     .outcome(TraceOutcome::Continue)
//!     .metadata(serde_json::json!({ "content": "hello" }))
//!     .finish()?;
//! writer.append(span);
//!
//! writer.close(EndReason::UserExit).await?;
//! # Ok(()) }
//! ```

#![warn(clippy::pedantic)]
// Identifiers like HKDF, HMAC, NDJSON appear frequently; backticking each
// inflates doc noise without improving legibility for this crate's audience.
#![allow(clippy::doc_markdown)]

pub mod chain;
pub mod entry;
pub mod error;
pub mod projection;
pub mod promotion;
pub mod query;
pub mod reader;
pub mod retention;
pub mod store;
pub mod writer;

pub use entry::{EntryKind, TurnEntry};
pub use error::{Result, TurnLogError};
pub use promotion::{
    HelixPromoter, PromotionCandidate, PromotionOutcome, PromotionReason, SiblingPromoter,
    promote_session, promote_session_with_pepper,
};
pub use reader::{ResumableSession, SessionSummary, TurnLogReader};
pub use store::StoreLayout;
pub use writer::{EndReason, TurnLogWriter};
