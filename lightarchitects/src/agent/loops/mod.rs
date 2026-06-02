//! L1 agentic loop runtime — Strategy trait, LoopRunner, and combinators.
//!
//! This module provides the 4-layer SDK agentic loop substrate:
//!
//! | Layer | Type | Location |
//! |-------|------|----------|
//! | L0 | [`LlmAgentProvider`] | `agent::provider` |
//! | L1 | [`Strategy`] + [`LoopRunner`] | **this module** |
//! | L2 | `ConversationSession` | `agent::session` |
//! | L3 | `WorkerPool` | `lightsquad::worker_pool` |
//!
//! ## Strategy classes
//!
//! Strategies fall into two classes based on their state/output types:
//!
//! ### Class A — Registered (L1 registry, shared `LoopState`/`LoopOutput`)
//!
//! Dispatchable via [`StrategyRegistry::lookup`]. All share the same
//! `State = LoopState` and `Output = LoopOutput` types.
//!
//! | Strategy | ID |
//! |----------|----|
//! | [`BuildStrategy`] | `"build"` |
//! | [`SecureStrategy`] | `"secure"` |
//! | [`ScrumStrategy`] | `"scrum"` |
//! | [`EnrichStrategy`] | `"enrich"` |
//! | [`GateStrategy`] | `"gate"` |
//! | [`ScopeGovernorStrategy`] | `"scope_governor"` |
//!
//! ### Class B — Custom (L0, own `State`/`Output` types)
//!
//! Require a caller-constructed executor. Not registered; use directly.
//!
//! | Strategy | Executor trait | Primary sibling |
//! |----------|---------------|-----------------|
//! | [`ReActStrategy`] | [`ReActExecutor`] | — |
//! | [`BcraStrategy`] | [`BcraExecutor`] | SERAPH |
//! | [`RedTeamStrategy`] | [`RedTeamExecutor`] | SERAPH |
//! | [`CoVeStrategy`] | [`CoVeExecutor`] | SERAPH / CORSO |
//! | [`IttStrategy`] | [`IttExecutor`] | QUANTUM |
//! | [`ReflexionStrategy`] | [`ReflexionExecutor`] | CORSO |
//! | [`MultiPassVerifyStrategy`] | [`MultiPassExecutor`] | CORSO |
//! | [`CritiqueRefineStrategy`] | — | — |
//! | [`DrainStrategy`] | [`DrainExecutor`] | CORSO |
//! | [`EnsembleStrategy`] | — | — |
//! | [`AchStrategy`] | [`AchExecutor`] | — |
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
pub mod bcra;
pub mod budget;
pub mod build;
pub mod compose;
pub mod convergence;
pub mod cove;
pub mod critique_refine;
pub mod drain;
pub mod enrich;
pub mod ensemble;
pub mod error;
pub mod gate;
pub mod itt;
pub mod meta_skill;
pub mod multipass;
pub mod profile;
pub mod react;
pub mod react_with_memory;
pub mod red_team;
pub mod reflexion;
pub mod registry;
pub mod runner;
pub mod sandbox_exec;
pub mod scope;
pub mod scope_governor;
pub mod scrum;
pub mod secure;
pub mod trace;

pub use ach::{
    AchExecutor, AchPhase, AchScoringEngine, AchState, AchStrategy, ConfidenceLevel,
    HypothesisTest, Prediction, TestResult, TestType,
};
pub use budget::Budget;
pub use compose::{Layered, Parallel, Race, Then};
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
pub use bcra::{BcraExecutor, BcraOutput, BcraPhase, BcraState, BcraStrategy};
pub use build::BuildStrategy;
pub use convergence::{
    BlastScore, ConvergenceGate, ConvergenceResult, InterestDecay, IntervalWatch, NPassVerifier,
    QueueDrain,
};
pub use drain::{DrainExecutor, DrainOutput, DrainState, DrainStrategy};
pub use enrich::EnrichStrategy;
pub use gate::{GatePhase, GateStrategy};
pub use meta_skill::{LoopOutput, LoopState, MetaSkill};
pub use multipass::{MultiPassExecutor, MultiPassOutput, MultiPassState, MultiPassVerifyStrategy};
pub use profile::{
    BudgetPolicy, ConcurrencyClass, DomainScopeConfig, LasdlcPhase, LoopProfile, PhaseOverride,
};
pub use react_with_memory::{MemoryStore, ReactWithMemoryState, ReactWithMemoryStrategy, RwmPhase};
pub use red_team::{RedTeamExecutor, RedTeamOutput, RedTeamPhase, RedTeamState, RedTeamStrategy};
pub use registry::{RegisteredStrategy, StrategyRegistry};
pub use sandbox_exec::{
    SandboxExecStrategy, SandboxExecutor, SandboxGenerator, SandboxPhase, SandboxPromotionRequest,
    SandboxState, SandboxTestResult, SandboxVerifier,
};
pub use scope::{DomainScopeResolver, ResolvedConfig};
pub use scope_governor::{ScopeGate, ScopeGovernorStrategy};
pub use scrum::{ScrumMode, ScrumStrategy};
pub use secure::SecureStrategy;
