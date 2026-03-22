//! `l-arc-arena` — plug-and-play training data factory for MCP tool-use LLMs.
//!
//! Users bring their own LLM endpoint and MCP servers. The arena:
//!
//! 1. **Auto-discovers** tool schemas from any MCP server (stdio or HTTP)
//! 2. **Generates** training exercises (7 types, 3 difficulty levels)
//! 3. **Executes** exercises against real servers, recording full traces
//! 4. **Scores** traces with an 8-dimensional reward system
//! 5. **Exports** SFT/DPO/RL-ready training data as JSONL
//!
//! # Quick Start
//!
//! ```bash
//! l-arc-arena run --config arena.yaml
//! l-arc-arena train --method sft --data ./training-data/
//! ```

/// Arena configuration parsing and validation.
pub mod config;
/// MCP server auto-discovery and tool registry.
pub mod discovery;
/// Execution engine: LLM client, server pool, tool routing.
pub mod engine;
/// Training exercise generation from tool schemas.
pub mod exercises;
/// Training data export (SFT, DPO, RL formats).
pub mod export;
/// Templatized prompt system for different training objectives.
pub mod prompts;
/// 8-dimensional reward scoring system.
#[allow(clippy::cast_precision_loss)] // Scoring counts are small; f64 precision is fine.
pub mod scoring;
