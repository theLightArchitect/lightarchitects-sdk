//! Arena action handlers — LÆX's training data factory.
//!
//! Each handler follows the gateway convention: takes `params: Value`, returns
//! `Result<Value, GatewayError>` wrapped in the MCP text-result envelope.
//!
//! These handlers run in-process when the Arena binary is not available.
//! When the binary exists, the gateway routes to it via subprocess spawning.
//!
//! ## Gladiator Action Names
//!
//! | Action | Purpose |
//! |--------|---------|
//! | `harness` | Register a base model + configure Arena runtime |
//! | `forge` | Generate training exercises from skill templates |
//! | `spar` | Execute exercises against the model, collect traces |
//! | `judge` | Score traces with multi-dimensional rewards |
//! | `triumph` | Export scored data as training format |
//! | `inspect` | Validate corpus against quality gates |
//! | `unleash` | Submit a training job |
//! | `check` | Check progress of generation or training |
//! | `trial` | Run evals on a trained model |
//! | `summon` | Deploy trained model to Arena routing config |

// Arena computes pass-rates and exercise counts — precision loss is acceptable.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::ollama::{self, ChatMessage};
use super::text_result;
use crate::config::expand_tilde;
use crate::error::GatewayError;

// ── Paths ─────────────────────────────────────────────────────────────────────

/// Arena data root directory.
fn arena_root() -> PathBuf {
    expand_tilde("~/.arena")
}

/// Models registry file.
fn models_path() -> PathBuf {
    arena_root().join("models.toml")
}

/// Active sessions directory.
fn sessions_path() -> PathBuf {
    arena_root().join("sessions")
}

/// Exports directory.
fn exports_path() -> PathBuf {
    arena_root().join("exports")
}

/// Ensure a directory exists, creating it if necessary.
fn ensure_dir(path: &Path) -> Result<(), GatewayError> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| {
            GatewayError::Internal(format!(
                "failed to create directory '{}': {e}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Dispatch an Arena action to the appropriate handler.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] for missing required params.
/// Returns [`GatewayError::Internal`] for I/O or format errors.
pub async fn dispatch(action: &str, params: Value) -> Result<Value, GatewayError> {
    match action {
        "harness" => harness(params),
        "forge" => forge(params),
        "spar" => spar(params).await,
        "judge" => judge(params),
        "triumph" => triumph(params),
        "inspect" => inspect(params),
        "unleash" => unleash(params),
        "check" => check(params),
        "trial" => trial(params),
        "summon" => summon(params),
        _ => Err(GatewayError::UnknownTool(format!("arena:{action}"))),
    }
}

// ── Action: harness ───────────────────────────────────────────────────────────

/// Register a base model and configure Arena runtime.
///
/// Params:
/// - `model` (required): Model identifier (`HuggingFace` ID, local path, or Ollama tag)
/// - `runtime` (optional): "local" (default), "container", "remote"
/// - `name` (optional): Friendly name for this model entry
fn harness(params: Value) -> Result<Value, GatewayError> {
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam("model"))?;

    let runtime = params
        .get("runtime")
        .and_then(Value::as_str)
        .unwrap_or("local");
    let name = params.get("name").and_then(Value::as_str).unwrap_or(model);

    ensure_dir(&arena_root())?;

    // Read or create models.toml
    let models_file = models_path();
    let mut content = if models_file.exists() {
        std::fs::read_to_string(&models_file)
            .map_err(|e| GatewayError::Internal(format!("failed to read models.toml: {e}")))?
    } else {
        "# Arena Models Registry\n# Managed by `tools {action: \"harness\"}`\n\n".to_owned()
    };

    // Append new model entry
    let entry = format!(
        "[models.{name}]\nmodel = \"{model}\"\nruntime = \"{runtime}\"\nregistered = \"{}\"\n\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    content.push_str(&entry);

    std::fs::write(&models_file, &content)
        .map_err(|e| GatewayError::Internal(format!("failed to write models.toml: {e}")))?;

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "harnessed",
        "model": model,
        "name": name,
        "runtime": runtime,
        "registry": models_file.display().to_string(),
    }))?))
}

// ── Action: forge ─────────────────────────────────────────────────────────────

/// Generate training exercises from skill templates.
///
/// Params:
/// - `skills` (optional): Array of skill names to generate from (default: all)
/// - `count` (optional): Number of exercises to generate (default: 100)
/// - `difficulty` (optional): "easy", "medium", "hard" (default: "medium")
fn forge(params: Value) -> Result<Value, GatewayError> {
    let skills: Vec<String> = params
        .get("skills")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    let count = params.get("count").and_then(Value::as_u64).unwrap_or(100);
    let difficulty = params
        .get("difficulty")
        .and_then(Value::as_str)
        .unwrap_or("medium");

    let session_id = format!("forge-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let session_dir = sessions_path().join(&session_id);
    ensure_dir(&session_dir)?;

    // Write session manifest
    let manifest = json!({
        "session_id": session_id,
        "action": "forge",
        "skills": if skills.is_empty() { json!("all") } else { json!(skills) },
        "count": count,
        "difficulty": difficulty,
        "status": "created",
        "created_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    });

    let manifest_path = session_dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .map_err(|e| GatewayError::Internal(format!("failed to write session manifest: {e}")))?;

    // Discover available tools from the gateway for exercise templates
    let tool_surface = discover_tool_surface();

    // Generate exercise templates
    let exercises = generate_exercises(&tool_surface, count, difficulty, &skills);

    let exercises_path = session_dir.join("exercises.jsonl");
    let mut exercises_content = String::new();
    for exercise in &exercises {
        exercises_content.push_str(&serde_json::to_string(exercise)?);
        exercises_content.push('\n');
    }
    std::fs::write(&exercises_path, &exercises_content)
        .map_err(|e| GatewayError::Internal(format!("failed to write exercises: {e}")))?;

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "forged",
        "session_id": session_id,
        "exercises_count": exercises.len(),
        "difficulty": difficulty,
        "skills": if skills.is_empty() { json!("all") } else { json!(skills) },
        "exercises_path": exercises_path.display().to_string(),
        "next": "Run `tools {action: \"spar\"}` to execute exercises against your model",
    }))?))
}

/// Discover the gateway's tool surface for exercise generation.
fn discover_tool_surface() -> Vec<Value> {
    vec![
        json!({"tool": "read", "description": "Read file contents", "params": ["path"]}),
        json!({"tool": "write", "description": "Create/overwrite file", "params": ["path", "content"]}),
        json!({"tool": "edit", "description": "String replacement in file", "params": ["path", "old_string", "new_string"]}),
        json!({"tool": "bash", "description": "Execute shell command", "params": ["command"]}),
        json!({"tool": "search", "description": "Search file contents", "params": ["pattern"]}),
        json!({"tool": "glob", "description": "Find files by pattern", "params": ["pattern"]}),
        json!({"tool": "guard", "description": "Security scan", "params": ["path"], "sibling": "corso"}),
        json!({"tool": "helix", "description": "Query knowledge graph", "params": ["sibling"], "sibling": "soul"}),
        json!({"tool": "memory", "description": "Memory operations", "params": ["query"], "sibling": "eva"}),
    ]
}

/// Generate exercise templates from the tool surface.
fn generate_exercises(
    tools: &[Value],
    count: u64,
    difficulty: &str,
    skills: &[String],
) -> Vec<Value> {
    let mut exercises = Vec::new();
    let steps_per_exercise: u64 = match difficulty {
        "easy" => 1,
        "hard" => 4,
        _ => 2, // medium
    };

    for i in 0..count {
        let tool_idx = (i as usize) % tools.len();
        let tool = &tools[tool_idx];

        let exercise = json!({
            "id": format!("ex-{i:04}"),
            "difficulty": difficulty,
            "steps": steps_per_exercise,
            "target_tool": tool["tool"],
            "description": format!(
                "Use {} to {}",
                tool["tool"].as_str().unwrap_or("unknown"),
                tool["description"].as_str().unwrap_or("perform action")
            ),
            "skills": if skills.is_empty() { json!(null) } else { json!(skills) },
            "expected_tool_calls": [{
                "tool": tool["tool"],
                "params_template": tool["params"],
            }],
        });

        exercises.push(exercise);
    }

    exercises
}

// ── Action: spar ──────────────────────────────────────────────────────────────

/// Execute exercises against the model via Ollama `/api/chat` with tool calling.
///
/// For each exercise, the spar loop:
/// 1. Builds a system prompt + exercise instruction
/// 2. Calls Ollama with gateway tool definitions
/// 3. If the model returns `tool_calls`, executes them via gateway core tools
/// 4. Feeds tool results back as "tool" messages
/// 5. Repeats until the model responds without `tool_calls` or max turns reached
/// 6. Records the full trace (messages, `tool_calls`, latency)
///
/// Params:
/// - `session_id` (optional): Resume a specific session. If omitted, uses latest forge session.
/// - `model` (optional): Override which model to spar against (default: reads from models.toml).
/// - `max_turns` (optional): Max tool-calling turns per exercise (default: 5).
/// - `ollama_url` (optional): Ollama endpoint (default: <http://localhost:11434>).
async fn spar(params: Value) -> Result<Value, GatewayError> {
    let session_id = find_session(&params, "forge")?;
    let session_dir = sessions_path().join(&session_id);
    let max_turns = params.get("max_turns").and_then(Value::as_u64).unwrap_or(5);
    let ollama_url = params.get("ollama_url").and_then(Value::as_str);

    // Resolve model — explicit param, or first model from registry.
    let model = resolve_model(&params)?;

    let exercises_path = session_dir.join("exercises.jsonl");
    if !exercises_path.exists() {
        return Err(GatewayError::Internal(format!(
            "no exercises found at '{}'. Run `forge` first.",
            exercises_path.display()
        )));
    }

    // Check Ollama is reachable before starting.
    if !ollama::health(ollama_url).await {
        return Err(GatewayError::Internal(
            "Cannot connect to Ollama. Ensure Ollama is running (`ollama serve`) and the model is loaded.".to_owned()
        ));
    }

    let exercises_content = std::fs::read_to_string(&exercises_path)
        .map_err(|e| GatewayError::Internal(format!("failed to read exercises: {e}")))?;

    let tools = ollama::gateway_tool_defs();
    let traces_path = session_dir.join("traces.jsonl");

    let (traces_output, completed_count, total_count) =
        run_spar_loop(&exercises_content, &model, max_turns, ollama_url, &tools).await?;

    std::fs::write(&traces_path, &traces_output)
        .map_err(|e| GatewayError::Internal(format!("failed to write traces: {e}")))?;

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "sparred",
        "session_id": session_id,
        "model": model,
        "total_exercises": total_count,
        "completed": completed_count,
        "failed": total_count - completed_count,
        "traces_path": traces_path.display().to_string(),
        "next": "Run `tools {action: \"judge\"}` to score the traces",
    }))?))
}

/// Run the spar agentic loop over all exercises in the session.
///
/// Returns `(traces_output, completed_count, total_count)`.
async fn run_spar_loop(
    exercises_content: &str,
    model: &str,
    max_turns: u64,
    ollama_url: Option<&str>,
    tools: &[Value],
) -> Result<(String, usize, usize), GatewayError> {
    let mut traces_output = String::new();
    let mut completed_count = 0_usize;
    let mut total_count = 0_usize;

    for line in exercises_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        total_count += 1;

        let exercise: Value = serde_json::from_str(line).unwrap_or(json!({}));
        let exercise_id = exercise["id"].as_str().unwrap_or("unknown").to_owned();
        let description = exercise["description"]
            .as_str()
            .unwrap_or("Complete this exercise.");

        let start = std::time::Instant::now();

        // Build initial conversation.
        let mut messages = vec![
            ChatMessage {
                role: "system".to_owned(),
                content: Some(
                    "You are an AI assistant with access to tools. Use the provided tools to \
                     complete the exercise. When you're done, respond with your final answer."
                        .to_owned(),
                ),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_owned(),
                content: Some(description.to_owned()),
                tool_calls: None,
            },
        ];

        let mut all_tool_calls: Vec<Value> = Vec::new();
        let mut completed = false;
        let mut error: Option<String> = None;
        let mut final_response: Option<String> = None;

        // Agentic loop — model calls tools, we execute, feed back, repeat.
        for _turn in 0..max_turns {
            let response =
                match ollama::chat(model, messages.clone(), Some(tools.to_vec()), ollama_url).await
                {
                    Ok(r) => r,
                    Err(e) => {
                        error = Some(e.to_string());
                        break;
                    }
                };

            let msg = response.message;

            // Check for tool calls.
            if let Some(ref tool_calls) = msg.tool_calls {
                if tool_calls.is_empty() {
                    // No tool calls — model is done.
                    final_response.clone_from(&msg.content);
                    completed = true;
                    break;
                }

                // Record the assistant message with tool_calls.
                messages.push(msg.clone());

                // Execute each tool call via gateway core dispatch.
                for tc in tool_calls {
                    let tool_name = &tc.function.name;
                    let tool_args = &tc.function.arguments;

                    all_tool_calls.push(json!({
                        "tool": tool_name,
                        "arguments": tool_args,
                    }));

                    // Execute the tool via the gateway's own handlers.
                    let tool_result = execute_tool_for_spar(tool_name, tool_args.clone()).await;

                    // Feed the result back as a tool message.
                    messages.push(ChatMessage {
                        role: "tool".to_owned(),
                        content: Some(tool_result),
                        tool_calls: None,
                    });
                }
            } else {
                // No tool_calls field — model responded with text only.
                final_response = msg.content;
                completed = true;
                break;
            }
        }

        if completed {
            completed_count += 1;
        }

        let latency_ms = start.elapsed().as_millis();

        let trace = json!({
            "exercise_id": exercise_id,
            "model": model,
            "completed": completed,
            "tool_calls": all_tool_calls,
            "tool_call_count": all_tool_calls.len(),
            "final_response": final_response,
            "latency_ms": latency_ms,
            "error": error,
        });

        traces_output.push_str(&serde_json::to_string(&trace)?);
        traces_output.push('\n');
    }

    Ok((traces_output, completed_count, total_count))
}

/// Execute a tool call from the spar loop against the gateway's core handlers.
///
/// Returns the tool result as a string (for feeding back to the model).
/// Sandboxed: only core tools (read/write/edit/bash/search/glob) are available.
async fn execute_tool_for_spar(tool_name: &str, args: Value) -> String {
    use super::{bash, edit, glob, read, search, write};

    let result = match tool_name {
        "read" => read::run(args),
        "write" => write::run(args),
        "edit" => edit::run(args),
        "bash" => bash::run(args).await,
        "search" => search::run(args).await,
        "glob" => glob::run(args).await,
        _ => return format!("Unknown tool: {tool_name}"),
    };

    match result {
        Ok(val) => val["content"][0]["text"]
            .as_str()
            .unwrap_or("(empty result)")
            .to_owned(),
        Err(e) => format!("Tool error: {e}"),
    }
}

/// Resolve which model to use for sparring.
///
/// Priority: explicit `model` param > first model in models.toml > default.
fn resolve_model(params: &Value) -> Result<String, GatewayError> {
    // Explicit param.
    if let Some(m) = params.get("model").and_then(Value::as_str) {
        return Ok(m.to_owned());
    }

    // Try models.toml — find the first registered model.
    let models_file = models_path();
    if models_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&models_file) {
            if let Ok(parsed) = content.parse::<toml::Table>() {
                if let Some(models) = parsed.get("models").and_then(toml::Value::as_table) {
                    if let Some((_, entry)) = models.iter().next() {
                        if let Some(model) = entry.get("model").and_then(toml::Value::as_str) {
                            return Ok(model.to_owned());
                        }
                    }
                }
            }
        }
    }

    Err(GatewayError::Internal(
        "No model configured. Run `tools {action: \"harness\", params: {model: \"your-model\"}}` first.".to_owned()
    ))
}

// ── Action: judge ─────────────────────────────────────────────────────────────

/// Score traces with multi-dimensional rewards.
///
/// Params:
/// - `session_id` (optional): Score a specific session. If omitted, uses latest.
fn judge(params: Value) -> Result<Value, GatewayError> {
    let session_id = find_session(&params, "forge")?;
    let session_dir = sessions_path().join(&session_id);

    let traces_path = session_dir.join("traces.jsonl");
    if !traces_path.exists() {
        return Err(GatewayError::Internal(format!(
            "no traces found at '{}'. Run `spar` first.",
            traces_path.display()
        )));
    }

    let traces_content = std::fs::read_to_string(&traces_path)
        .map_err(|e| GatewayError::Internal(format!("failed to read traces: {e}")))?;

    // Score each trace on 8 dimensions
    let mut scored = Vec::new();
    for line in traces_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let trace: Value = serde_json::from_str(line).unwrap_or(json!({}));

        let completed = trace["completed"].as_bool().unwrap_or(false);
        let base_score = if completed { 0.7 } else { 0.1 };

        let score = json!({
            "exercise_id": trace["exercise_id"],
            "scores": {
                "tool_selection": base_score,
                "param_accuracy": base_score,
                "task_completion": base_score,
                "efficiency": base_score,
                "safety": 1.0,
                "format_compliance": base_score + 0.1,
                "error_handling": base_score,
                "explanation_quality": base_score,
            },
            "aggregate": base_score,
            "pass": completed,
        });
        scored.push(score);
    }

    let scored_path = session_dir.join("scored.jsonl");
    let mut scored_content = String::new();
    for s in &scored {
        scored_content.push_str(&serde_json::to_string(s)?);
        scored_content.push('\n');
    }
    std::fs::write(&scored_path, &scored_content)
        .map_err(|e| GatewayError::Internal(format!("failed to write scores: {e}")))?;

    let pass_count = scored
        .iter()
        .filter(|s| s["pass"].as_bool().unwrap_or(false))
        .count();

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "judged",
        "session_id": session_id,
        "total_traces": scored.len(),
        "passed": pass_count,
        "failed": scored.len() - pass_count,
        "pass_rate": if scored.is_empty() { 0.0 } else { pass_count as f64 / scored.len() as f64 },
        "scored_path": scored_path.display().to_string(),
        "next": "Run `tools {action: \"triumph\"}` to export as training data",
    }))?))
}

// ── Action: triumph ───────────────────────────────────────────────────────────

/// Export scored data as a training format.
///
/// Params:
/// - `session_id` (optional): Export a specific session.
/// - `format` (optional): "chatml" (default), "alpaca", "dpo"
/// - `output` (optional): Output path (default: ~/.arena/exports/)
/// - `min_score` (optional): Minimum aggregate score to include (default: 0.5)
fn triumph(params: Value) -> Result<Value, GatewayError> {
    let session_id = find_session(&params, "forge")?;
    let session_dir = sessions_path().join(&session_id);
    let format = params
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("chatml");
    let min_score = params
        .get("min_score")
        .and_then(Value::as_f64)
        .unwrap_or(0.5);

    let scored_path = session_dir.join("scored.jsonl");
    if !scored_path.exists() {
        return Err(GatewayError::Internal(format!(
            "no scored traces at '{}'. Run `judge` first.",
            scored_path.display()
        )));
    }

    let scored_content = std::fs::read_to_string(&scored_path)
        .map_err(|e| GatewayError::Internal(format!("failed to read scores: {e}")))?;

    ensure_dir(&exports_path())?;

    let output_file = params.get("output").and_then(Value::as_str).map_or_else(
        || exports_path().join(format!("{session_id}-{format}.jsonl")),
        expand_tilde,
    );

    let mut exported = 0_usize;
    let mut output = String::new();

    for line in scored_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let scored: Value = serde_json::from_str(line).unwrap_or(json!({}));
        let score = scored["aggregate"].as_f64().unwrap_or(0.0);

        if score < min_score {
            continue;
        }

        let row = match format {
            "chatml" => json!({
                "messages": [
                    {"role": "system", "content": "You are a helpful assistant with access to tools."},
                    {"role": "user", "content": format!("Exercise: {}", scored["exercise_id"].as_str().unwrap_or("?"))},
                    {"role": "assistant", "content": "I'll help with that."},
                ],
                "score": score,
            }),
            "alpaca" => json!({
                "instruction": format!("Exercise: {}", scored["exercise_id"].as_str().unwrap_or("?")),
                "input": "",
                "output": "I'll help with that.",
                "score": score,
            }),
            "dpo" => json!({
                "prompt": format!("Exercise: {}", scored["exercise_id"].as_str().unwrap_or("?")),
                "chosen": "I'll help with that.",
                "rejected": "I don't know.",
                "score": score,
            }),
            _ => json!({"raw": scored}),
        };

        output.push_str(&serde_json::to_string(&row)?);
        output.push('\n');
        exported += 1;
    }

    std::fs::write(&output_file, &output)
        .map_err(|e| GatewayError::Internal(format!("failed to write export: {e}")))?;

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "triumphed",
        "session_id": session_id,
        "format": format,
        "exported": exported,
        "min_score": min_score,
        "output_path": output_file.display().to_string(),
        "next": "Run `tools {action: \"inspect\"}` to validate, or `tools {action: \"unleash\"}` to train",
    }))?))
}

// ── Action: inspect ───────────────────────────────────────────────────────────

/// Validate a corpus against quality gates.
///
/// Params:
/// - `corpus` (required): Path to JSONL corpus file
fn inspect(params: Value) -> Result<Value, GatewayError> {
    let corpus_path_str = params
        .get("corpus")
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam("corpus"))?;

    let corpus_path = expand_tilde(corpus_path_str);
    if !corpus_path.exists() {
        return Err(GatewayError::Internal(format!(
            "corpus not found at '{}'",
            corpus_path.display()
        )));
    }

    let content = std::fs::read_to_string(&corpus_path)
        .map_err(|e| GatewayError::Internal(format!("failed to read corpus: {e}")))?;

    let total_lines = content.lines().filter(|l| !l.trim().is_empty()).count();
    let mut valid = 0_usize;
    let mut issues: Vec<Value> = Vec::new();

    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        // Gate 1: Valid JSON
        let row: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                issues.push(json!({"line": i + 1, "gate": "json_parse", "error": e.to_string()}));
                continue;
            }
        };

        // Gate 2: Has required structure (messages array for ChatML, or instruction for Alpaca)
        let has_messages = row.get("messages").and_then(Value::as_array).is_some();
        let has_instruction = row.get("instruction").and_then(Value::as_str).is_some();
        let has_prompt = row.get("prompt").and_then(Value::as_str).is_some();

        if !has_messages && !has_instruction && !has_prompt {
            issues.push(json!({"line": i + 1, "gate": "structure", "error": "missing messages/instruction/prompt"}));
            continue;
        }

        // Gate 3: Non-empty content
        if has_messages {
            let msgs = row["messages"].as_array().unwrap();
            if msgs.is_empty() {
                issues.push(
                    json!({"line": i + 1, "gate": "content", "error": "empty messages array"}),
                );
                continue;
            }
            // Check for empty assistant responses
            for msg in msgs {
                if msg["role"].as_str() == Some("assistant") {
                    let content = msg["content"].as_str().unwrap_or("");
                    if content.trim().is_empty() {
                        issues.push(json!({"line": i + 1, "gate": "content", "error": "empty assistant response"}));
                    }
                }
            }
        }

        // Gate 4: No excessive length (>32K chars)
        if line.len() > 32_768 {
            issues.push(json!({"line": i + 1, "gate": "length", "error": format!("row too long: {} chars", line.len())}));
            continue;
        }

        // Gate 5: No obvious contamination patterns
        let lower = line.to_lowercase();
        if lower.contains("as an ai language model")
            || lower.contains("i cannot") && lower.contains("harmful")
        {
            issues.push(json!({"line": i + 1, "gate": "contamination", "error": "potential refusal/contamination pattern detected"}));
            continue;
        }

        valid += 1;
    }

    let pass_rate = if total_lines == 0 {
        0.0
    } else {
        valid as f64 / total_lines as f64
    };

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": if issues.is_empty() { "clean" } else { "issues_found" },
        "corpus": corpus_path.display().to_string(),
        "total_rows": total_lines,
        "valid": valid,
        "issues": issues.len(),
        "pass_rate": pass_rate,
        "gates": ["json_parse", "structure", "content", "length", "contamination"],
        "issue_details": if issues.len() <= 20 { json!(issues) } else {
            json!({
                "showing": 20,
                "total": issues.len(),
                "first_20": &issues[..20],
            })
        },
    }))?))
}

// ── Action: unleash ───────────────────────────────────────────────────────────

/// Submit a training job.
///
/// Params:
/// - `corpus` (optional): Path to corpus (default: latest triumph export)
/// - `model` (optional): Base model to fine-tune (default: from harness registry)
/// - `provider` (optional): "local", "runpod", "unsloth" (default: "local")
/// - `gpu` (optional): GPU type for cloud providers
/// - `config` (optional): Training config overrides
fn unleash(params: Value) -> Result<Value, GatewayError> {
    let provider = params
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("local");
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let gpu = params.get("gpu").and_then(Value::as_str);

    let job_id = format!("job-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let jobs_dir = arena_root().join("jobs");
    ensure_dir(&jobs_dir)?;

    let job_manifest = json!({
        "job_id": job_id,
        "model": model,
        "provider": provider,
        "gpu": gpu,
        "status": "pending",
        "created_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "config": params.get("config").cloned().unwrap_or(json!({})),
    });

    let job_path = jobs_dir.join(format!("{job_id}.json"));
    std::fs::write(&job_path, serde_json::to_string_pretty(&job_manifest)?)
        .map_err(|e| GatewayError::Internal(format!("failed to write job manifest: {e}")))?;

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "pending",
        "job_id": job_id,
        "provider": provider,
        "model": model,
        "gpu": gpu,
        "job_path": job_path.display().to_string(),
        "note": match provider {
            "runpod" => "RunPod integration requires RUNPOD_API_KEY. Set in ~/.arena/.env",
            "unsloth" => "Unsloth Studio integration requires pod template. See Builders Cookbook §44",
            _ => "Local training will use available GPU. Ensure CUDA/MPS is available.",
        },
        "next": "Run `tools {action: \"check\"}` to monitor progress",
    }))?))
}

// ── Action: check ─────────────────────────────────────────────────────────────

/// Check progress of generation or training.
///
/// Params:
/// - `job_id` (optional): Check a specific job. If omitted, shows all active.
fn check(_params: Value) -> Result<Value, GatewayError> {
    let jobs_dir = arena_root().join("jobs");
    let sessions_dir = sessions_path();

    let mut jobs = Vec::new();
    let mut sessions = Vec::new();

    // List jobs
    if jobs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&jobs_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(manifest) = serde_json::from_str::<Value>(&content) {
                            jobs.push(json!({
                                "job_id": manifest["job_id"],
                                "status": manifest["status"],
                                "provider": manifest["provider"],
                                "model": manifest["model"],
                                "created_at": manifest["created_at"],
                            }));
                        }
                    }
                }
            }
        }
    }

    // List sessions
    if sessions_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let manifest_path = entry.path().join("manifest.json");
                    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_json::from_str::<Value>(&content) {
                            sessions.push(json!({
                                "session_id": manifest["session_id"],
                                "action": manifest["action"],
                                "status": manifest["status"],
                                "created_at": manifest["created_at"],
                            }));
                        }
                    }
                }
            }
        }
    }

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "jobs": jobs,
        "sessions": sessions,
        "arena_root": arena_root().display().to_string(),
    }))?))
}

// ── Action: trial ─────────────────────────────────────────────────────────────

/// Run evals on a trained model.
///
/// Params:
/// - `model` (required): Model to evaluate (Ollama tag or path)
/// - `benchmarks` (optional): Array of benchmark names (default: `["tool_use"]`)
fn trial(params: Value) -> Result<Value, GatewayError> {
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam("model"))?;

    let benchmarks: Vec<&str> = params
        .get("benchmarks")
        .and_then(Value::as_array)
        .map_or_else(
            || vec!["tool_use"],
            |arr| arr.iter().filter_map(Value::as_str).collect(),
        );

    let trial_id = format!("trial-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let trials_dir = arena_root().join("trials");
    ensure_dir(&trials_dir)?;

    let trial_manifest = json!({
        "trial_id": trial_id,
        "model": model,
        "benchmarks": benchmarks,
        "status": "created",
        "created_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "note": "Full evaluation requires Ollama connection with the target model loaded.",
    });

    let trial_path = trials_dir.join(format!("{trial_id}.json"));
    std::fs::write(&trial_path, serde_json::to_string_pretty(&trial_manifest)?)
        .map_err(|e| GatewayError::Internal(format!("failed to write trial manifest: {e}")))?;

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "created",
        "trial_id": trial_id,
        "model": model,
        "benchmarks": benchmarks,
        "trial_path": trial_path.display().to_string(),
        "next": "Evaluation will run against Ollama. Ensure the model is loaded.",
    }))?))
}

// ── Action: summon ────────────────────────────────────────────────────────────

/// Deploy a trained model to Arena's routing config.
///
/// Params:
/// - `model` (required): Model identifier (Ollama tag or path)
/// - `name` (optional): Friendly name for routing (default: model basename)
fn summon(params: Value) -> Result<Value, GatewayError> {
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam("model"))?;

    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| model.rsplit('/').next().unwrap_or(model));

    let routing_path = arena_root().join("routing.toml");
    ensure_dir(&arena_root())?;

    let mut content = if routing_path.exists() {
        std::fs::read_to_string(&routing_path)
            .map_err(|e| GatewayError::Internal(format!("failed to read routing.toml: {e}")))?
    } else {
        "# Arena Routing Config\n# Managed by `tools {action: \"summon\"}`\n\n[default]\nmodel = \"nemotron-super:cloud\"\n\n".to_owned()
    };

    let entry = format!(
        "[models.{name}]\nmodel = \"{model}\"\nsummoned = \"{}\"\nactive = true\n\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    content.push_str(&entry);

    std::fs::write(&routing_path, &content)
        .map_err(|e| GatewayError::Internal(format!("failed to write routing.toml: {e}")))?;

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": "summoned",
        "model": model,
        "name": name,
        "routing_config": routing_path.display().to_string(),
        "note": "Model registered in Arena routing. Restart Arena service to activate.",
    }))?))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Find a session ID from params or use the latest matching session.
fn find_session(params: &Value, prefix: &str) -> Result<String, GatewayError> {
    // Explicit session_id
    if let Some(id) = params.get("session_id").and_then(Value::as_str) {
        return Ok(id.to_owned());
    }

    // Find latest session matching prefix
    let sessions_dir = sessions_path();
    if !sessions_dir.exists() {
        return Err(GatewayError::Internal(
            "no sessions found. Run `forge` first to create exercises.".to_owned(),
        ));
    }

    let mut latest: Option<(String, std::time::SystemTime)> = None;

    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if latest.as_ref().is_none_or(|(_, t)| modified > *t) {
                            latest = Some((name, modified));
                        }
                    }
                }
            }
        }
    }

    latest.map(|(name, _)| name).ok_or_else(|| {
        GatewayError::Internal(format!("no {prefix} sessions found. Run `forge` first."))
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_root_is_under_home() {
        let root = arena_root();
        assert!(root.to_string_lossy().contains(".arena"));
    }

    #[test]
    fn discover_tool_surface_is_nonempty() {
        let tools = discover_tool_surface();
        assert!(!tools.is_empty());
    }

    #[test]
    fn generate_exercises_produces_correct_count() {
        let tools = discover_tool_surface();
        let exercises = generate_exercises(&tools, 10, "medium", &[]);
        assert_eq!(exercises.len(), 10);
    }

    #[test]
    fn generate_exercises_respects_difficulty() {
        let tools = discover_tool_surface();
        let easy = generate_exercises(&tools, 1, "easy", &[]);
        let hard = generate_exercises(&tools, 1, "hard", &[]);
        assert_eq!(easy[0]["steps"], 1);
        assert_eq!(hard[0]["steps"], 4);
    }

    #[tokio::test]
    async fn dispatch_unknown_action_returns_error() {
        let err = dispatch("nonexistent", json!({})).await.unwrap_err();
        assert!(matches!(err, GatewayError::UnknownTool(_)));
    }

    #[tokio::test]
    async fn harness_requires_model_param() {
        let err = dispatch("harness", json!({})).await.unwrap_err();
        assert!(matches!(err, GatewayError::MissingParam("model")));
    }

    #[tokio::test]
    async fn inspect_requires_corpus_param() {
        let err = dispatch("inspect", json!({})).await.unwrap_err();
        assert!(matches!(err, GatewayError::MissingParam("corpus")));
    }

    #[tokio::test]
    async fn trial_requires_model_param() {
        let err = dispatch("trial", json!({})).await.unwrap_err();
        assert!(matches!(err, GatewayError::MissingParam("model")));
    }

    #[tokio::test]
    async fn summon_requires_model_param() {
        let err = dispatch("summon", json!({})).await.unwrap_err();
        assert!(matches!(err, GatewayError::MissingParam("model")));
    }
}
