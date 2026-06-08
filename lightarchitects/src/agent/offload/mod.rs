//! LightSquad Mesh-Aware Offload Provider.
//!
//! [`OffloadAwareProvider`] wraps an existing [`crate::agent::LlmAgentProvider`]
//! and routes catalog-matched, no-tool-use requests through the
//! [`laex_supervisor::OffloadDispatcher`] to a cheap-tier specialist model
//! (default: `glm-5.1:cloud` via local Ollama at `:11434/v1`). Tool-using
//! turns and non-matched requests pass through to the wrapped provider.
//!
//! # Module map
//!
//! | Module | Day | Purpose |
//! |---|---|---|
//! | [`catalog`] | 1 | Pattern allowlist YAML schema + loader |
//! | [`charter`] | 2 | Sibling persona/charter registry (8 siblings) |
//! | [`context`] | 3-4 | `ContextResolver` trait + Helix/Canon/IndustryBaseline impls |
//! | [`validator`] | 5 | Mechanical [`ShapeValidator`] |
//! | [`refiner`] | 5 | [`PromptRefiner`] for shape and LÆX retries |
//! | [`laex_supervisor`] | 6 | `LaexSupervisor` + `OffloadDispatcher` trait |
//! | [`hitl_bridge`] | 7 | `HitlEscalator` trait + null + Ironclaw impls |
//! | [`dispatch`] | 9-10 | [`LiteLLMHttpDispatcher`] (`OpenAI`-compat HTTP) |
//! | [`provider`] | 9-10 | [`OffloadAwareProvider`] |
//! | (prompt_builder) | 11 | Deferred — token-budgeted prompt assembly + P3 anchor extraction |
//!
//! # Quick wiring (when `lightsquad` feature is off)
//!
//! ```ignore
//! use std::collections::HashMap;
//! use std::sync::Arc;
//! use lightarchitects::agent::offload::{
//!     LiteLLMHttpDispatcher, LaexSupervisor, NullEscalator,
//!     OffloadAwareProvider, OffloadCatalog,
//! };
//!
//! let catalog = Arc::new(OffloadCatalog::load_from_helix()?);
//! let dispatcher = Arc::new(LiteLLMHttpDispatcher::from_catalog(&catalog)?);
//! let supervisor = Arc::new(LaexSupervisor::new(catalog.clone(), dispatcher.clone()));
//! let provider = OffloadAwareProvider::new(
//!     Arc::new(inner_provider),
//!     catalog,
//!     HashMap::new(),                // wire HelixSource/CanonSource/IndustryBaselineSource keyed by kind_str()
//!     dispatcher,
//!     supervisor,
//!     Arc::new(NullEscalator),
//! );
//! ```

pub mod catalog;
pub mod charter;
pub mod context;
pub mod dispatch;
pub mod hitl_bridge;
pub mod laex_supervisor;
pub mod prompt_builder;
pub mod provider;
pub mod refiner;
pub mod validator;

// ─── Re-exports (Days 1-10 ergonomic surface) ────────────────────────────────

pub use catalog::{
    Calibration, CatalogError, ClassificationHint, ContextSource, ContextSourceOverlay,
    Eligibility, OffloadCatalog, Pattern, Refinement, Shape, Verifier,
};
pub use charter::{SiblingCharter, SiblingCharterRegistry};
pub use context::{
    CanonSource, ContextError, ContextResolver, HelixQueryRunner, HelixSource,
    IndustryBaselineSource, ResolvedContext, slice_by_anchor_prefix,
};
pub use dispatch::LiteLLMHttpDispatcher;
#[cfg(feature = "lightsquad")]
pub use hitl_bridge::IronclawEscalator;
pub use hitl_bridge::{
    BridgeError, EscalationRequest, EscalationResolution, HitlEscalator, NullEscalator,
};
pub use laex_supervisor::{
    LaexSupervisor, OffloadDispatcher, SupervisorError, SupervisorVerdict, VerifierContext,
};
pub use prompt_builder::{
    AssembledPrompt, BudgetConfig, ComponentTokenUsage, PromptBuilderError,
    assemble as assemble_prompt, extract_rendered_anchor, render_template,
};
pub use provider::OffloadAwareProvider;
pub use refiner::PromptRefiner;
pub use validator::{ShapeValidator, ShapeViolation};
