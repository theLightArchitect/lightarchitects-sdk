//! React-loop dispatch path for the copilot.
//!
//! `dispatch_react_turn` is the conversational analog of `dispatch_strategy_initial`:
//! instead of running a single named strategy directly, it spins up a `ReactStrategy`
//! with an LLM-backed executor (`LlmReActExecutor`) that can invoke Class A
//! strategies as tools via `StrategyToolExecutor`. The LLM provides per-turn
//! continuity by accumulating prior thoughts/actions/observations in the
//! `ReActPrompt.steps[]` scratchpad.
//!
//! Per SCRUM 2026-06-02:
//! - Default tool allowlist excludes `gate` and `scope_governor` (SERAPH VETO C2)
//! - Symmetric `IndirectInjectionShield` runs on tool outputs (SERAPH VETO C1)
//! - `loop.dispatch` AYIN span emitted with actor="copilot-react" per tool call
//! - `HelixSessionMemory` provides multi-turn memory bridge (P2 continuity)
//!
//! ## Path predicate
//!
//! Triggered when the operator message starts with `/react` (case-insensitive)
//! OR when the build session has `react_mode` enabled.

use std::path::PathBuf;
use std::time::Instant;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures_util::StreamExt as _;
use lightarchitects::agent::ChainContext;
use lightarchitects::agent::ClaudeCliProvider;
use lightarchitects::agent::conversation::HelixSessionMemory;
use lightarchitects::agent::conversation::memory::ConversationMemory as _;
use lightarchitects::agent::loops::llm_executor::LlmReActExecutor;
use lightarchitects::agent::loops::{
    Budget, LoopRunner, Outcome, ReActPrompt, ReActStrategy, StrategyToolExecutor,
};
use uuid::Uuid;

use crate::events::{WebEvent, WebEventV2};

/// Maximum `ReAct` steps per copilot turn (cost + latency cap).
const REACT_MAX_STEPS: usize = 7;

/// Trigger predicate for the react path.
#[must_use]
pub fn should_route_to_react(message: &str) -> bool {
    let lower = message.trim_start().to_lowercase();
    lower.starts_with("/react")
}

/// Strip the `/react` prefix from a message, returning the underlying query.
fn strip_react_prefix(message: &str) -> String {
    let trimmed = message.trim_start();
    let lower = trimmed.to_lowercase();
    if lower.starts_with("/react") {
        // Remove "/react" (6 chars) and any leading whitespace after.
        trimmed[6..].trim_start().to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Dispatch a react-loop copilot turn.
///
/// Spawns the loop on a background task and streams iteration summaries through
/// the build session's SSE broadcast channel. Returns a JSON acknowledgement.
///
/// ## Provider
///
/// Uses [`ClaudeCliProvider::default()`] which spawns `claude -p` as a subprocess
/// inheriting the host CLI's OAuth session (Claude Code subscription). The
/// provider explicitly removes `ANTHROPIC_API_KEY` from the subprocess env so
/// no direct API billing path is ever invoked from this code path.
///
/// To use an explicit API key instead, construct a custom provider via the
/// `ClaudeCliProvider` builder fields (`api_key`, `default_model`, etc.) and
/// thread it through this function — see `agent::claude` module docs.
#[allow(clippy::too_many_lines, clippy::unused_async)]
pub async fn dispatch_react_turn(
    build_id: Uuid,
    message: &str,
    cwd: PathBuf,
    turn_span_id: String,
    event_tx: tokio::sync::broadcast::Sender<WebEventV2>,
) -> axum::response::Response {
    let query = strip_react_prefix(message);
    if query.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": "empty react query",
                "hint": "use /react <task description>"
            })),
        )
            .into_response();
    }

    // Provider: Claude Code subscription via `claude` CLI OAuth.
    // No Anthropic API key billed; the subprocess env_remove("ANTHROPIC_API_KEY")
    // guarantees the host CLI's stored OAuth tokens are the only auth path.
    let provider = ClaudeCliProvider::default();

    // Open the session memory; falls back to in-memory if helix path unavailable.
    let memory = HelixSessionMemory::open(&cwd, 10);

    let span_id_for_response = turn_span_id.clone();
    let event_tx_for_task = event_tx.clone();
    let query_for_task = query.clone();

    // Spawn the loop on a background task so the HTTP response can return
    // immediately. Progress streams through SSE via WebEventV2::CopilotResponse.
    tokio::spawn(async move {
        const MAX_CONSECUTIVE_NO_TOOL: u32 = 2;
        let tool_executor = StrategyToolExecutor::new_default();
        let llm_executor = LlmReActExecutor::new(provider, tool_executor);
        let strategy = ReActStrategy::new(llm_executor).with_name("react-copilot");
        let runner = LoopRunner::new(strategy, Budget::unlimited());

        let initial = ReActPrompt::new(&query_for_task, REACT_MAX_STEPS);
        let session_id = build_id.to_string();
        let chain = ChainContext::default();

        // Emit dispatch span for the react loop itself.
        let dispatch_start = Instant::now();
        lightarchitects::agent::loops::trace::emit_dispatch(
            "copilot-react",
            "react",
            Some("orchestrator"),
            Some("research"),
            dispatch_start,
        );

        tracing::info!(build_id = %build_id, "react loop spawning stream");
        let mut stream = runner.run(initial, chain, Some(session_id.clone()));
        let mut iterations: u32 = 0;
        let mut last_thought = String::new();
        let mut final_summary = String::new();
        let mut consecutive_no_tool: u32 = 0;

        while let Some(step_result) = stream.next().await {
            iterations += 1;
            tracing::info!(build_id = %build_id, iteration = iterations, "react iteration produced");
            let result = match step_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(build_id = %build_id, error = %e, "react loop error");
                    let _ = event_tx_for_task.send(WebEventV2::from_event(
                        WebEvent::CopilotResponse {
                            chunk: format!("[react] error: {e}"),
                            done: true,
                            sibling: Some("copilot-react".to_owned()),
                            turn_span_id: Some(turn_span_id.clone()),
                        },
                        None,
                    ));
                    return;
                }
            };

            match result.outcome {
                Outcome::Continue(prompt) => {
                    if let Some(step) = prompt.steps.last() {
                        last_thought.clone_from(&step.thought);
                        // Stagnation detection: count consecutive no-tool steps.
                        // Models that ignore the tools array produce text-only
                        // steps — the loop is "advancing" but not doing useful
                        // ReAct work. Halt early so the operator sees feedback.
                        if step.action == "no-tool" {
                            consecutive_no_tool += 1;
                        } else {
                            consecutive_no_tool = 0;
                        }
                        let chunk = format_iteration_chunk(iterations, step);
                        let _ = event_tx_for_task.send(WebEventV2::from_event(
                            WebEvent::CopilotResponse {
                                chunk,
                                done: false,
                                sibling: Some("copilot-react".to_owned()),
                                turn_span_id: Some(turn_span_id.clone()),
                            },
                            None,
                        ));
                        if consecutive_no_tool >= MAX_CONSECUTIVE_NO_TOOL {
                            tracing::warn!(
                                build_id = %build_id,
                                iterations,
                                "react loop halted — model emitted no tool calls for {} consecutive steps",
                                consecutive_no_tool
                            );
                            final_summary = format!(
                                "[react halted — no-tool stagnation after {iterations} iterations]\n\nFinal reasoning: {last_thought}"
                            );
                            break;
                        }
                    }
                }
                Outcome::Halt(final_prompt) => {
                    final_summary = build_final_summary(&final_prompt, &last_thought);
                    break;
                }
                Outcome::Pause(_, hitl) => {
                    final_summary = format!("[react paused] {}", hitl.question);
                    break;
                }
            }
        }

        if final_summary.is_empty() {
            "[react] no response generated".clone_into(&mut final_summary);
        }

        // Persist the final agent turn to HelixSessionMemory.
        let mut mem = memory;
        mem.push(
            lightarchitects::agent::conversation::MessageRole::User,
            query_for_task,
        );
        mem.push(
            lightarchitects::agent::conversation::MessageRole::Assistant,
            final_summary.clone(),
        );

        let _ = event_tx_for_task.send(WebEventV2::from_event(
            WebEvent::CopilotResponse {
                chunk: final_summary,
                done: true,
                sibling: Some("copilot-react".to_owned()),
                turn_span_id: Some(turn_span_id.clone()),
            },
            None,
        ));
    });

    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "react_dispatched",
            "build_id": build_id.to_string(),
            "turn_span_id": span_id_for_response,
        })),
    )
        .into_response()
}

fn format_iteration_chunk(
    iteration: u32,
    step: &lightarchitects::agent::loops::ReActStep,
) -> String {
    let mut s = format!("[react #{iteration} {}] action={}", step.phase, step.action);
    if !step.thought.is_empty() {
        s.push_str("\nthought: ");
        s.push_str(&truncate(&step.thought, 512));
    }
    if !step.observation.is_empty() {
        s.push_str("\nobservation: ");
        s.push_str(&truncate(&step.observation, 1024));
    }
    s
}

fn build_final_summary(prompt: &ReActPrompt, last_thought: &str) -> String {
    use std::fmt::Write as _;
    let mut s = format!(
        "[react complete] {} iterations, phase {}\n\n",
        prompt.steps.len(),
        prompt.phase
    );
    if !last_thought.is_empty() {
        s.push_str("Final reasoning: ");
        s.push_str(last_thought);
    }
    if !prompt.steps.is_empty() {
        s.push_str("\n\nSteps taken:\n");
        for (i, step) in prompt.steps.iter().enumerate() {
            let _ = writeln!(s, "  {}. [{}] {}", i + 1, step.phase, step.action);
        }
    }
    s
}

fn truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...[truncated]", &s[..end])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn route_predicate_matches_slash_react() {
        assert!(should_route_to_react("/react audit auth"));
        assert!(should_route_to_react("/REACT audit auth"));
        assert!(should_route_to_react("  /react audit"));
        assert!(!should_route_to_react("react audit"));
        assert!(!should_route_to_react("/secure scan"));
        assert!(!should_route_to_react(""));
    }

    #[test]
    fn strip_prefix_removes_slash_react() {
        assert_eq!(strip_react_prefix("/react audit auth"), "audit auth");
        assert_eq!(strip_react_prefix("/REACT audit"), "audit");
        assert_eq!(strip_react_prefix("/react   spaced"), "spaced");
        assert_eq!(strip_react_prefix("/react"), "");
    }

    #[test]
    fn truncate_respects_byte_cap() {
        let s = "a".repeat(100);
        let t = truncate(&s, 10);
        assert!(t.starts_with("aaaaaaaaaa"));
        assert!(t.contains("truncated"));
    }
}
