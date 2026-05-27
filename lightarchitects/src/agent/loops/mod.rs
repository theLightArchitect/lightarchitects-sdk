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

pub mod ach;
pub mod budget;
pub mod build;
pub mod compose;
pub mod cove;
pub mod critique_refine;
pub mod enrich;
pub mod ensemble;
pub mod error;
pub mod itt;
pub mod meta_skill;
pub mod phase_span;
pub mod react;
pub mod reflexion;
pub mod registry;
pub mod runner;
pub mod scrum;
pub mod secure;
pub mod trace;

pub use ach::{
    AchExecutor, AchPhase, AchScoringEngine, AchState, AchStrategy, ConfidenceLevel,
    HypothesisTest, Prediction, TestResult, TestType,
};
pub use budget::Budget;
pub use compose::{Layered, Parallel, Then};
pub use cove::{
    ClaimCategory, CoVeExecutor, CoVePhase, CoVeResult, CoVeState, CoVeStrategy,
    VerificationStatus, VerifiedClaim,
};
pub use critique_refine::CritiqueRefineStrategy;
pub use ensemble::{EnsembleState, EnsembleStrategy};
pub use error::LoopError;
pub use itt::{
    EvidenceRef, InvestigationTaskTree, IttExecutor, IttStrategy, NodeId, QPhase, TreeNode,
    VerificationResult,
};
pub use react::{ReActExecutor, ReActPhase, ReActPrompt, ReActStep, ReActStrategy};
pub use reflexion::{
    ReflexionEntry, ReflexionExecutor, ReflexionLoopState, ReflexionState, ReflexionStrategy,
};
pub use runner::{HitlRequest, LoopRunner, Outcome, StepContext, StepResult, Strategy};

// Chatroom strategy loops (Phase 4)
pub use build::BuildStrategy;
pub use enrich::EnrichStrategy;
pub use meta_skill::{LoopOutput, LoopState, MetaSkill};
pub use registry::{RegisteredStrategy, StrategyRegistry};
pub use scrum::{ScrumMode, ScrumStrategy};
pub use secure::SecureStrategy;
