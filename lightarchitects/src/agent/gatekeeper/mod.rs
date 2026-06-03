//! Stateless gate-evaluator substrate (`[A+S+Q+C+O+P+K+D+T+R]` LASDLC dimensions).
//!
//! # Design
//!
//! A [`Gatekeeper`] is a **pure-function evaluator** that critiques a
//! [`Draft`] against [`Criteria`] and returns a [`Verdict`]. Implementations
//! MUST be stateless by construction:
//!
//! - No `&mut self` on the trait method
//! - No interior mutability (`Mutex`, `RwLock`, `RefCell`, `Cell`, `Atomic*`,
//!   `UnsafeCell`) on the struct
//! - No global state access; no disk I/O outside what's reachable through
//!   `criteria`; only network calls through the LLM provider passed at
//!   construction
//!
//! Memory lives in the **canon + helix** layer ([`assembler::CriteriaAssembler`]
//! retrieves relevant precedent at call time), not in the gatekeeper instance.
//! Same `(draft, criteria) → Verdict`, always (modulo LLM nondeterminism —
//! pin temperature=0 for full determinism).
//!
//! # Citation invariant
//!
//! Every [`Finding`] returned in a non-refusal [`Verdict`] must carry at least
//! one [`Citation`]. [`Verdict::try_new`] enforces this at construction time;
//! a parsed LLM response with a citation-less finding is rejected as
//! [`GateError::FindingWithoutCitation`].
//!
//! # Refusal invariant
//!
//! When [`CriteriaAssembler`] returns fewer than
//! [`Gatekeeper::min_criteria_completeness`] reference entries, the gatekeeper
//! MUST emit a [`VerdictStatus::RetrievalInsufficient`] verdict instead of
//! issuing a thin-context judgment.
//!
//! # See also
//!
//! - Canon XXXIII (independent verification)
//! - Canon XXXV (citation gate)
//! - Anthropic 2024 "Building Effective Agents" — Evaluator-Optimizer workflow
//! - `standards/research/agent-loop-patterns-catalogue.md`

pub mod assembler;
pub mod canon;
pub mod quality;
pub mod trait_def;
pub mod types;

pub use trait_def::Gatekeeper;
pub use types::{
    BaselineRef, CanonRef, Citation, Criteria, Draft, DraftKind, DraftLocation, Finding,
    GateDimension, GateError, HelixSnapshotId, PlanRef, PrecedentRef, Severity, Verdict,
    VerdictStatus,
};

pub use assembler::{AssemblerConfig, AssemblyError, CriteriaAssembler, CriteriaSource};
pub use canon::CanonGatekeeper;
pub use quality::QualityGatekeeper;
