//! L1 agentic loop runtime — Strategy trait, LoopRunner, and combinators.
//!
//! This module provides the 4-layer SDK agentic loop substrate:
//!
//! | Layer | Type | Location |
//! |-------|------|----------|
//! | L0 | [`LlmAgentProvider`] | `agent::provider` |
//! | L1 | [`Strategy`] + [`LoopRunner`] | **this module** |
//! | L2 | `ConversationSession` | `agent::session` (Phase 3) |
//! | L3 | `WorkerPool` | `lightsquad::worker_pool` (Phase 5) |
//!
//! # Quick start
//!
//! ```rust,no_run
//! use lightarchitects::agent::loops::{LoopRunner, Outcome, Strategy, StepContext};
//! use lightarchitects::agent::loops::error::LoopError;
//! use lightarchitects::agent::loops::budget::Budget;
//! use lightarchitects::agent::ChainContext;
//! use async_trait::async_trait;
//!
//! struct Echo;
//!
//! #[async_trait]
//! impl Strategy for Echo {
//!     type State = String;
//!     type Output = String;
//!
//!     async fn step(&self, s: String, _ctx: &StepContext) -> Result<Outcome<String, String>, LoopError> {
//!         Ok(Outcome::Halt(s))
//!     }
//!     fn name(&self) -> &'static str { "Echo" }
//! }
//!
//! # async fn run() {
//! use futures_util::StreamExt as _;
//! let mut stream = LoopRunner::new(Echo, Budget::unlimited())
//!     .run("hello".to_owned(), ChainContext::default(), None);
//! while let Some(step) = stream.next().await { let _ = step; }
//! # }
//! ```
//!
//! [`LlmAgentProvider`]: crate::agent::LlmAgentProvider

pub mod budget;
pub mod compose;
pub mod critique_refine;
pub mod error;
pub mod phase_span;
pub mod runner;
pub mod trace;

pub use budget::Budget;
pub use compose::{Layered, Parallel, Then};
pub use critique_refine::CritiqueRefineStrategy;
pub use error::LoopError;
pub use runner::{LoopRunner, Outcome, StepContext, StepResult, Strategy};
