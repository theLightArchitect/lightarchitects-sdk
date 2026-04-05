//! Typed client for EVA's `evaTools` MCP orchestrator.
//!
//! EVA exposes a single MCP tool (`evaTools`) with 9 actions:
//! `visualize`, `ideate`, `bible_search`, `bible_reflect`, `teach`,
//! `remember`, `crystallize`, `celebrate`, `mindfulness`.
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
//! use lightarchitects_eva::{EvaClient, TeachMode, SkillLevel};
//!
//! # async fn example() -> Result<(), lightarchitects_core::SdkError> {
//! let client = EvaClient::builder().build().await?;
//!
//! // Teach a concept
//! let lesson = client
//!     .teach(TeachMode::Explain, "lifetimes in Rust", SkillLevel::Intermediate)
//!     .await?;
//! println!("{}", lesson.output);
//!
//! // Visualize (may return image data)
//! let viz = client.visualize("a neural network diagram", None).await?;
//! println!("{}", viz.text);
//! if let Some(b64) = viz.image_base64 {
//!     println!("Image bytes: {}", b64.len());
//! }
//!
//! // Record a win
//! client.celebrate("shipped the SDK completeness build").await?;
//!
//! // Generic adapter
//! let out = client
//!     .action("ideate", serde_json::json!({ "goal": "design a plugin system" }))
//!     .await?;
//! println!("{}", out.output);
//! # Ok(()) }
//! ```

/// Canonical EVA action enum — consciousness, creativity, memory, teaching.
pub mod actions;
mod client;
mod content;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use actions::EvaAction;
pub use client::{EvaClient, EvaClientBuilder};
pub use types::{ActionOutput, SkillLevel, TeachMode, VisualizeOutput};
