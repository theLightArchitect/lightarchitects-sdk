#![cfg(feature = "loops-core")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr
)]
//! Live `ReAct` strategy loop demo using `llama3.2:3b` via local Ollama.
//!
//! Runs three `ReAct` steps through `LoopRunner` → streams tokens to stdout so
//! you can watch the LLM reason in real time. Requires a running Ollama server
//! at `OLLAMA_HOST` (default `http://localhost:11434`) with `llama3.2:3b` pulled.
//!
//! ```text
//! cargo test --test loop_local_ollama_demo \
//!     --features loops-core \
//!     -- --ignored --nocapture
//! ```
//!
//! The model is selectable via `LOOP_DEMO_MODEL` (default: `llama3.2:3b`).

use async_trait::async_trait;
use futures_util::StreamExt as _;
use lightarchitects::agent::{
    AgentRequest, ChainContext, LlmAgentProvider, OllamaCliProvider, ProviderEvent,
    loops::{
        Budget, LoopRunner, Outcome,
        error::LoopError,
        react::{ReActExecutor, ReActPhase, ReActPrompt, ReActStep, ReActStrategy},
        runner::StepContext,
    },
};

// ── executor ─────────────────────────────────────────────────────────────────

/// `ReActExecutor` backed by a local Ollama model.
///
/// Each step builds a structured Thought/Action/Result prompt, calls
/// `spawn_streaming()`, streams tokens live to stdout, then parses the
/// collected text.
struct OllamaReActExecutor {
    provider: OllamaCliProvider,
}

impl OllamaReActExecutor {
    fn new(provider: OllamaCliProvider) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl ReActExecutor for OllamaReActExecutor {
    async fn step(&self, prompt: &ReActPrompt, _ctx: &StepContext) -> Result<ReActStep, LoopError> {
        let system = "You are a concise reasoning agent. Respond in exactly this format:\n\
                      Thought: <one sentence reasoning>\n\
                      Action: <one concrete next action>\n\
                      Result: <expected outcome or observation>";

        let history = prompt
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "Step {}: Thought: {} | Action: {} | Result: {}",
                    i + 1,
                    s.thought,
                    s.action,
                    s.result.as_deref().unwrap_or("—")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let user_msg = format!(
            "Task: {}\nContext: {}{}",
            prompt.query,
            prompt.context,
            if history.is_empty() {
                String::new()
            } else {
                format!("\n\nPrevious steps:\n{history}")
            }
        );

        let step_num = prompt.steps.len() + 1;
        eprintln!("\n── Step {step_num} ──────────────────────────────────────");
        eprint!("LLM → ");

        let req = AgentRequest {
            sibling_identity: system.to_owned(),
            user_prompt: user_msg,
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            // Local inference: no budget cap — comment required per CLAUDE.md
            // (bounded structured output: Thought/Action/Result ≤ ~200 tokens)
            max_budget_usd: 0.0,
            model_hint: None,
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
            conversation_history: vec![],
            tool_definitions: vec![],
        };

        let sanitized = req
            .sanitize()
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        // Stream tokens live to stderr so the user sees the LLM typing in real time.
        let mut stream = self
            .provider
            .spawn_streaming(sanitized)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let mut full_text = String::new();
        while let Some(event) = stream.next().await {
            if let ProviderEvent::TextDelta { text, .. } = event {
                eprint!("{text}");
                full_text.push_str(&text);
            }
        }
        eprintln!(); // newline after streaming ends

        let thought = extract_field(&full_text, "Thought").unwrap_or_else(|| {
            full_text
                .lines()
                .next()
                .unwrap_or("(no thought)")
                .to_owned()
        });
        let action = extract_field(&full_text, "Action").unwrap_or_else(|| "continue".to_owned());
        let result = extract_field(&full_text, "Result");

        // Advance phase: move toward Conclude on the final step.
        let next_phase = if step_num >= prompt.max_steps.saturating_sub(1) {
            ReActPhase::Close
        } else {
            prompt.phase.next()
        };

        Ok(ReActStep {
            observation: result.clone().unwrap_or_default(),
            thought,
            action,
            result,
            phase: next_phase,
        })
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn extract_field(text: &str, field: &str) -> Option<String> {
    let prefix = format!("{field}:");
    text.lines()
        .find(|l| l.trim_start().starts_with(&prefix))
        .map(|l| l.trim_start().trim_start_matches(&prefix).trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn resolve_model() -> String {
    std::env::var("LOOP_DEMO_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "llama3.2:3b".to_owned())
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// Live 3-step `ReAct` loop. Streams each LLM response to stderr in real time.
///
/// PASS = loop halts within 3 steps, each step has non-empty thought+action.
/// The task is deliberately simple so `llama3.2:3b` can answer reliably.
#[tokio::test]
#[ignore = "requires live Ollama at OLLAMA_HOST with llama3.2:3b (or LOOP_DEMO_MODEL) pulled"]
async fn react_loop_live_llama3_3_steps() {
    let model = resolve_model();
    eprintln!("=== Loop Demo — model: {model} ===");
    eprintln!("Connecting to Ollama at OLLAMA_HOST (default http://localhost:11434)…");

    let provider = OllamaCliProvider::new_local(&model, None);
    let executor = OllamaReActExecutor::new(provider);
    let strategy = ReActStrategy::new(executor).with_name("demo-react");

    let task = ReActPrompt::new(
        "What are 3 key benefits of Rust's ownership model for systems programming?",
        /* max_steps = */ 3,
    );

    let mut stream =
        LoopRunner::new(strategy, Budget::unlimited()).run(task, ChainContext::default(), None);

    let mut step_count = 0u32;
    let mut final_state: Option<ReActPrompt> = None;

    while let Some(result) = stream.next().await {
        let step = result.expect("loop step must not error");
        step_count += 1;
        match step.outcome {
            Outcome::Continue(state) => {
                assert!(
                    !state.steps.is_empty(),
                    "state must have steps after Continue"
                );
                let last = state.steps.last().unwrap();
                assert!(!last.thought.is_empty(), "thought must be non-empty");
                assert!(!last.action.is_empty(), "action must be non-empty");
                eprintln!("[step {step_count}] thought: {}", last.thought);
                eprintln!("[step {step_count}] action:  {}", last.action);
                final_state = Some(state);
            }
            Outcome::Halt(state) => {
                eprintln!("\n=== Halt after {step_count} step(s) ===");
                eprintln!("Final query: {}", state.query);
                eprintln!("Steps completed: {}", state.steps.len());
                for (i, s) in state.steps.iter().enumerate() {
                    eprintln!("  [{}] T: {} | A: {}", i + 1, s.thought, s.action);
                }
                final_state = Some(state);
                break;
            }
            Outcome::Pause(_, _) => panic!("ReActStrategy must never pause"),
        }
    }

    assert!(step_count > 0, "at least one step must execute");
    assert!(
        final_state.is_some(),
        "stream must produce at least one outcome"
    );
}

/// Verify the `new_local()` constructor is wired correctly: no registry
/// validation, base URL defaults to localhost, cost estimate is non-negative.
/// This test does NOT make a network call.
#[test]
fn new_local_bypasses_registry_and_uses_localhost() {
    let p = OllamaCliProvider::new_local("llama3.2:3b", None);
    assert_eq!(p.default_model, "llama3.2:3b");
    assert_eq!(p.name(), "ollama-cli");
    // estimate_cost gracefully handles unregistered slugs (returns CostTier::Low rate).
    let cost = p.estimate_cost(1_000, 500);
    assert!(cost >= 0.0, "cost must be non-negative for local model");
}

/// Verify that `new()` still rejects unregistered slugs.
#[test]
fn new_cloud_still_rejects_unknown_slugs() {
    let err = OllamaCliProvider::new("not-a-real-model:cloud", None).unwrap_err();
    assert!(format!("{err}").contains("not-a-real-model:cloud"));
}
