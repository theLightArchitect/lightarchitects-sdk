//! Typed client for EVA's `evaTools` MCP orchestrator.
//!
//! EVA exposes a single MCP tool (`evaTools`) with 8 actions — `visualize`,
//! `ideate`, `memory`, `build`, `research`, `bible`, `secure`, and `teach` —
//! matching the orchestrator pattern used by CORSO (`corsoTools`) and QUANTUM
//! (`qsTools`).
//!
//! This crate provides two call paths:
//!
//! - **Typed methods** — one method per action, with fully-typed parameter enums
//!   and structured return types. Use these when the action is known at compile time.
//! - **Generic adapter** — [`EvaClient::action`] accepts any action name and raw
//!   JSON params, returning [`ActionOutput`]. Use this for dynamic dispatch or
//!   when building higher-level orchestration layers.
//!
//! Responses from EVA are wrapped in the MCP `ToolCallResult` content-block
//! format. This crate transparently unwraps that envelope before returning
//! values to callers.
//!
//! # Quick start
//!
//! ```no_run
//! use lightarchitects_eva::{EvaClient, BuildMode, TeachMode, SkillLevel, ResearchSource};
//!
//! # async fn example() -> Result<(), lightarchitects_core::SdkError> {
//! let client = EvaClient::builder().build().await?;
//!
//! // Typed method: teach a concept
//! let lesson = client
//!     .teach(TeachMode::Explain, "lifetimes in Rust", SkillLevel::Intermediate)
//!     .await?;
//! println!("{}", lesson.output);
//!
//! // Typed method: code review
//! let review = client
//!     .build(BuildMode::Review, Some("fn foo() { panic!() }"), Some("rust"))
//!     .await?;
//! println!("{}", review.output);
//!
//! // Typed method: visualize (returns image data when EVA generates one)
//! let viz = client.visualize("a neural network diagram", None).await?;
//! println!("{}", viz.text);
//! if let Some(b64) = viz.image_base64 {
//!     println!("Image bytes: {}", b64.len());
//! }
//!
//! // Generic adapter: call any EVA tool by name
//! let out = client
//!     .action("ideate", serde_json::json!({ "goal": "design a plugin system" }))
//!     .await?;
//! println!("{}", out.output);
//! # Ok(()) }
//! ```

mod client;
mod content;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use client::{EvaClient, EvaClientBuilder};
pub use types::{
    ActionOutput, BibleAction, BuildMode, MemorySubcommand, ResearchSource, SecureAction,
    SkillLevel, TeachMode, VisualizeOutput,
};
