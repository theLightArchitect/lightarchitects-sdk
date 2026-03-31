//! `lightarchitects-oracle` — Multi-model mathematical verification oracle.
//!
//! Dispatches mathematical claims to multiple AI models in parallel, each bringing
//! a different analytical lens. The consensus (or disagreement) across models provides
//! higher confidence than any single model alone.
//!
//! # Architecture
//!
//! ```text
//! OracleClient::prove("Is this bound tight?")
//!   ├──→ Leanstral (Mistral API)      → Lean 4 formal proof
//!   ├──→ DeepSeek V3.2 (Ollama Cloud) → Step-by-step derivation
//!   └──→ Qwen 3.5 (Ollama Cloud)      → Numerical bounds + edge cases
//!         │
//!         ▼
//!   OracleVerdict { consensus, findings, proofs }
//! ```
//!
//! # Models
//!
//! | Model | Role | Strength |
//! |-------|------|----------|
//! | Leanstral | `formal_proof` | Machine-checkable Lean 4 proofs |
//! | `DeepSeek` | `derivation` | Step-by-step mathematical reasoning |
//! | Qwen | `numerical` | Bounds checking, counterexample search |
//! | Kimi | `reasoning` | Deep thinking, edge case analysis |
//! | Cogito | `reasoning` | Structured analysis with confidence |
//!
//! # Usage
//!
//! ```rust,no_run
//! use lightarchitects_oracle::{OracleClient, OracleMode};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let oracle = OracleClient::builder()
//!     .ollama_endpoint("http://localhost:11434")
//!     .build()?;
//!
//! let verdict = oracle
//!     .query("Prove: |round(x) - x| <= 0.5 for all real x")
//!     .mode(OracleMode::Prove)
//!     .call()
//!     .await?;
//!
//! for finding in &verdict.findings {
//!     println!("{}: {:?} ({:?})", finding.model, finding.status, finding.role);
//! }
//! println!("Consensus: {:?}", verdict.consensus);
//! # Ok(())
//! # }
//! ```

mod client;
mod models;
mod verdict;

pub use client::{OracleClient, OracleClientBuilder, OracleQuery};
pub use models::{ModelId, ModelRole, OracleMode};
pub use verdict::{Consensus, Finding, OracleVerdict};
