//! L3 orchestration — bounded-concurrency pool and circuit-breaker supervisor.
//!
//! Lifted from `lightsquad::wave_dispatcher` and generalised beyond the
//! git-worktree domain. Enabled by the `loops-core` feature alongside the
//! L1/L2 substrate.
//!
//! # Layer placement
//!
//! ```text
//! L3: Orchestration  ← this module (WorkerPool / Supervisor)
//! L2: Conversation   ← ConversationSession, Transport, Hooks
//! L1: Loops          ← Strategy, LoopRunner, EnsembleStrategy
//! L0: Providers      ← LlmAgentProvider (Phase 6)
//! ```

mod supervisor;
mod worker_pool;

pub use supervisor::{Supervisor, SupervisorResult};
pub use worker_pool::{DEFAULT_CAPACITY, WorkerPool};
