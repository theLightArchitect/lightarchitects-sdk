//! Worker spawn — wraps `crate::agent::ClaudeCliProvider` for autonomous worker pool.
//!
//! Per canonical IRONCLAW PDF spec (7-Slot Agent Pool §):
//! ```text
//! claude --bare -p "{task_prompt}" --allowedTools "Read,Edit,Write,Bash" --output-format json
//! ```
//! - `--bare` skips CLAUDE.md auto-scan; context injected explicitly via `--append-system-prompt-file`
//! - `ANTHROPIC_API_KEY` set per worker tier (Sonnet / Haiku / Ollama Cloud)
//! - 3-5s startup overhead; negligible for tasks running 5-30 minutes
//! - 7 concurrent slots during peak wave execution
//! - Slot 1 becomes ReviewGate during gate cycle; other slots idle
//!
//! Worker tier allocation (peak):
//! - SLOT 1-2: Sonnet (complex impl)
//! - SLOT 3:   Ollama Cloud (qwen3-coder:480b or deepseek-v3.1:671b)
//! - SLOT 4-7: Haiku (simple edits, test boilerplate, formatting)
//!
//! Phase 3 implementation — wraps `crate::agent::ClaudeCliProvider` (already
//! implements subprocess spawn + G1 `sanitize_params`); adds slot allocator,
//! tier router (per `crate::lightsquad::decision_pipeline::ModelRouter`),
//! and result-channel routing back to `crate::lightsquad::wave_dispatcher`.
//!
//! Phase 1 stub — slot pool declared in Phase 3.
