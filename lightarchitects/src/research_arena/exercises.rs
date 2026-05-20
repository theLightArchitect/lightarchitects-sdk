//! Training exercise generation from discovered tool schemas.
//!
//! Generates 7 types of exercises at 3 difficulty levels from auto-discovered
//! MCP tool schemas. Exercise generation is deterministic (seeded RNG) and
//! covers all discovered tools.

use crate::core::action::ToolInfo;
use rand::Rng;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::research_arena::config::{Difficulty, ExerciseType};
use crate::research_arena::discovery::ToolRegistry;

/// A single training exercise generated from tool schemas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    /// Unique exercise identifier.
    pub id: String,
    /// Exercise type.
    pub exercise_type: ExerciseType,
    /// Difficulty level.
    pub difficulty: Difficulty,
    /// Natural-language task description for the model.
    pub task: String,
    /// Context provided to the model (e.g., prior conversation, data).
    pub context: String,
    /// Available tools (subset of registry for this exercise).
    pub available_tools: Vec<ToolInfo>,
    /// Expected correct answer for scoring.
    pub expected: ExpectedAnswer,
    /// Which server(s) this exercise targets.
    pub target_servers: Vec<String>,
    /// Tool names that must NOT be called (safety scoring).
    ///
    /// An empty list means no tools are forbidden for this exercise.
    #[serde(default)]
    pub forbidden_tools: Vec<String>,
}

/// Expected answer for scoring an exercise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedAnswer {
    /// Expected tool calls in order (empty if no tool call expected).
    pub tool_calls: Vec<ExpectedToolCall>,
    /// Whether a tool call is expected at all.
    pub expects_tool_call: bool,
    /// Optional expected final answer pattern.
    #[serde(default)]
    pub answer_contains: Option<String>,
}

/// An expected tool call for scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedToolCall {
    /// Tool name.
    pub tool_name: String,
    /// Server that owns this tool.
    pub server_name: String,
    /// Expected parameter keys (values may vary).
    pub expected_params: Vec<String>,
}

/// Exercise generation configuration derived from arena config.
pub struct GeneratorConfig {
    /// Types to generate.
    pub types: Vec<ExerciseType>,
    /// Difficulty levels to include.
    pub difficulties: Vec<Difficulty>,
    /// Total exercise count.
    pub count: u32,
    /// Random seed for deterministic generation.
    pub seed: u64,
}

/// Generate exercises from a tool registry.
///
/// Returns a deterministic set of exercises based on the seed. All discovered
/// tools are covered — the generator ensures every tool appears in at least
/// one exercise.
///
/// # Errors
///
/// Returns an error string if the registry is empty or constraints are
/// unsatisfiable (e.g., cross-server with only one server).
pub fn generate(
    registry: &ToolRegistry,
    config: &GeneratorConfig,
) -> Result<Vec<Exercise>, String> {
    if registry.tool_count() == 0 {
        return Err("cannot generate exercises: no tools discovered".into());
    }

    let mut rng = rand::rngs::StdRng::seed_from_u64(config.seed);
    let mut exercises = Vec::with_capacity(config.count as usize);
    let all_tools = registry.all_tools();
    let server_names: Vec<&str> = registry.server_names().collect();

    let per_type = distribute_count(config.count, config.types.len());

    for (i, exercise_type) in config.types.iter().enumerate() {
        let type_count = per_type[i];
        let per_diff = distribute_count(type_count, config.difficulties.len());

        for (j, difficulty) in config.difficulties.iter().enumerate() {
            let diff_count = per_diff[j];
            for k in 0..diff_count {
                let id = format!(
                    "{}-{}-{}",
                    exercise_type_slug(*exercise_type),
                    difficulty_slug(*difficulty),
                    k
                );

                let exercise = generate_one(
                    &id,
                    *exercise_type,
                    *difficulty,
                    &all_tools,
                    &server_names,
                    registry,
                    &mut rng,
                )?;

                exercises.push(exercise);
            }
        }
    }

    Ok(exercises)
}

/// Generate a single exercise.
fn generate_one(
    id: &str,
    exercise_type: ExerciseType,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    server_names: &[&str],
    _registry: &ToolRegistry,
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    match exercise_type {
        ExerciseType::ToolSelection => generate_tool_selection(id, difficulty, all_tools, rng),
        ExerciseType::ParameterFilling => {
            generate_parameter_filling(id, difficulty, all_tools, rng)
        }
        ExerciseType::MultiStepChain => generate_multi_step(id, difficulty, all_tools, rng),
        ExerciseType::ErrorRecovery => generate_error_recovery(id, difficulty, all_tools, rng),
        ExerciseType::Distractor => generate_distractor(id, difficulty, all_tools, rng),
        ExerciseType::NoToolNeeded => generate_no_tool(id, difficulty, all_tools, rng),
        ExerciseType::CrossServer => {
            generate_cross_server(id, difficulty, all_tools, server_names, rng)
        }
    }
}

// ── Generator implementations ───────────────────────────────────────────────

fn generate_tool_selection(
    id: &str,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    if all_tools.is_empty() {
        return Err("no tools available for tool-selection exercise".into());
    }

    // Pick a target tool.
    let target_idx = rng.gen_range(0..all_tools.len());
    let (server, target_tool) = &all_tools[target_idx];

    // Build available tools set: target + distractors based on difficulty.
    let distractor_count = match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Medium => 3,
        Difficulty::Hard => 6.min(all_tools.len().saturating_sub(1)),
    };

    let mut available: Vec<ToolInfo> = vec![(*target_tool).clone()];
    let mut used_indices = vec![target_idx];

    for _ in 0..distractor_count {
        if used_indices.len() >= all_tools.len() {
            break;
        }
        let mut idx = rng.gen_range(0..all_tools.len());
        while used_indices.contains(&idx) {
            idx = (idx + 1) % all_tools.len();
        }
        used_indices.push(idx);
        available.push(all_tools[idx].1.clone());
    }

    available.shuffle(rng);

    let desc = target_tool
        .description
        .as_deref()
        .unwrap_or("a tool operation");

    let task = format!(
        "You need to perform the following task: {desc}. \
         Select the most appropriate tool from the available options."
    );

    let expected_params = extract_required_params(&target_tool.input_schema);

    Ok(Exercise {
        id: id.to_owned(),
        exercise_type: ExerciseType::ToolSelection,
        difficulty,
        task,
        context: String::new(),
        available_tools: available,
        expected: ExpectedAnswer {
            tool_calls: vec![ExpectedToolCall {
                tool_name: target_tool.name.clone(),
                server_name: (*server).to_owned(),
                expected_params,
            }],
            expects_tool_call: true,
            answer_contains: None,
        },
        target_servers: vec![(*server).to_owned()],
        forbidden_tools: vec![],
    })
}

fn generate_parameter_filling(
    id: &str,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    if all_tools.is_empty() {
        return Err("no tools for parameter-filling".into());
    }

    // Prefer tools that declare parameters; fall back to all tools when none qualify.
    let tools_with_params: Vec<(&str, &ToolInfo)> = all_tools
        .iter()
        .filter(|(_, t)| has_properties(&t.input_schema))
        .copied()
        .collect();

    let (server, tool): &(&str, &ToolInfo) = if tools_with_params.is_empty() {
        let idx = rng.gen_range(0..all_tools.len());
        &all_tools[idx]
    } else {
        let idx = rng.gen_range(0..tools_with_params.len());
        &tools_with_params[idx]
    };

    let params = extract_required_params(&tool.input_schema);
    let param_desc = if params.is_empty() {
        "appropriate parameters".to_owned()
    } else {
        params.join(", ")
    };

    let context_detail = match difficulty {
        Difficulty::Easy => "All required information is provided below.",
        Difficulty::Medium => "Some information must be inferred from the context.",
        Difficulty::Hard => {
            "Information is scattered across the context. Some parameters have constraints."
        }
    };

    let task = format!(
        "Call the '{}' tool with the correct parameters ({}). {}",
        tool.name, param_desc, context_detail
    );

    Ok(Exercise {
        id: id.to_owned(),
        exercise_type: ExerciseType::ParameterFilling,
        difficulty,
        task,
        context: format!("Tool schema: {}", tool.input_schema),
        available_tools: vec![(*tool).clone()],
        expected: ExpectedAnswer {
            tool_calls: vec![ExpectedToolCall {
                tool_name: tool.name.clone(),
                server_name: (*server).to_owned(),
                expected_params: params,
            }],
            expects_tool_call: true,
            answer_contains: None,
        },
        target_servers: vec![(*server).to_owned()],
        forbidden_tools: vec![],
    })
}

fn generate_multi_step(
    id: &str,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    let step_count = match difficulty {
        Difficulty::Easy => 2,
        Difficulty::Medium => 3,
        Difficulty::Hard => 4.min(all_tools.len()),
    };

    if all_tools.len() < step_count {
        return Err(format!(
            "need at least {step_count} tools for multi-step at this difficulty"
        ));
    }

    let mut indices: Vec<usize> = (0..all_tools.len()).collect();
    indices.shuffle(rng);
    let selected: Vec<_> = indices[..step_count]
        .iter()
        .map(|&i| &all_tools[i])
        .collect();

    let steps_desc: Vec<String> = selected
        .iter()
        .enumerate()
        .map(|(i, (_, tool))| {
            let desc = tool
                .description
                .as_deref()
                .unwrap_or("perform an operation");
            format!("Step {}: {desc}", i + 1)
        })
        .collect();

    let task = format!("Complete this multi-step task:\n{}", steps_desc.join("\n"));

    let tool_calls: Vec<ExpectedToolCall> = selected
        .iter()
        .map(|(server, tool)| ExpectedToolCall {
            tool_name: tool.name.clone(),
            server_name: (*server).to_owned(),
            expected_params: extract_required_params(&tool.input_schema),
        })
        .collect();

    let servers: Vec<String> = selected.iter().map(|(s, _)| (*s).to_owned()).collect();

    let available: Vec<ToolInfo> = selected.iter().map(|(_, t)| (*t).clone()).collect();

    Ok(Exercise {
        id: id.to_owned(),
        exercise_type: ExerciseType::MultiStepChain,
        difficulty,
        task,
        context: String::new(),
        available_tools: available,
        expected: ExpectedAnswer {
            tool_calls,
            expects_tool_call: true,
            answer_contains: None,
        },
        target_servers: servers,
        forbidden_tools: vec![],
    })
}

fn generate_error_recovery(
    id: &str,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    if all_tools.len() < 2 {
        return Err("need at least 2 tools for error-recovery".into());
    }

    let idx_a = rng.gen_range(0..all_tools.len());
    let mut idx_b = rng.gen_range(0..all_tools.len());
    while idx_b == idx_a {
        idx_b = (idx_b + 1) % all_tools.len();
    }

    let (server_a, tool_a) = &all_tools[idx_a];
    let (server_b, tool_b) = &all_tools[idx_b];

    let error_desc = match difficulty {
        Difficulty::Easy => "The tool returned an error.",
        Difficulty::Medium => "The tool timed out after 30 seconds.",
        Difficulty::Hard => {
            "The tool returned a partial result with missing fields. \
             You must decide whether to retry, use an alternative, or work with partial data."
        }
    };

    let task = format!(
        "You tried to use '{}' but it failed. {error_desc} \
         Find an alternative approach using the available tools.",
        tool_a.name
    );

    Ok(Exercise {
        id: id.to_owned(),
        exercise_type: ExerciseType::ErrorRecovery,
        difficulty,
        task,
        context: format!("Failed tool: {}", tool_a.name),
        available_tools: vec![(*tool_a).clone(), (*tool_b).clone()],
        expected: ExpectedAnswer {
            tool_calls: vec![ExpectedToolCall {
                tool_name: tool_b.name.clone(),
                server_name: (*server_b).to_owned(),
                expected_params: extract_required_params(&tool_b.input_schema),
            }],
            expects_tool_call: true,
            answer_contains: None,
        },
        target_servers: vec![(*server_a).to_owned(), (*server_b).to_owned()],
        forbidden_tools: vec![],
    })
}

fn generate_distractor(
    id: &str,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    if all_tools.len() < 2 {
        return Err("need at least 2 tools for distractor exercise".into());
    }

    // Pick 1–2 relevant tools and add distractors.
    let relevant_count = match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Medium | Difficulty::Hard => 2.min(all_tools.len()),
    };

    let distractor_count = match difficulty {
        Difficulty::Easy => 2,
        Difficulty::Medium => 4,
        Difficulty::Hard => 6,
    }
    .min(all_tools.len().saturating_sub(relevant_count));

    let mut indices: Vec<usize> = (0..all_tools.len()).collect();
    indices.shuffle(rng);

    let relevant: Vec<_> = indices[..relevant_count]
        .iter()
        .map(|&i| &all_tools[i])
        .collect();
    let distractors: Vec<_> = indices[relevant_count..relevant_count + distractor_count]
        .iter()
        .map(|&i| &all_tools[i])
        .collect();

    let relevant_desc: Vec<String> = relevant
        .iter()
        .map(|(_, t)| {
            t.description
                .as_deref()
                .unwrap_or("perform an operation")
                .to_owned()
        })
        .collect();

    let task = format!(
        "Given the following task(s), select ONLY the relevant tool(s). \
         Ignore tools that are not needed.\nTasks: {}",
        relevant_desc.join("; ")
    );

    let mut available: Vec<ToolInfo> = relevant
        .iter()
        .chain(distractors.iter())
        .map(|(_, t)| (*t).clone())
        .collect();
    available.shuffle(rng);

    let tool_calls: Vec<ExpectedToolCall> = relevant
        .iter()
        .map(|(server, tool)| ExpectedToolCall {
            tool_name: tool.name.clone(),
            server_name: (*server).to_owned(),
            expected_params: extract_required_params(&tool.input_schema),
        })
        .collect();

    let servers: Vec<String> = relevant.iter().map(|(s, _)| (*s).to_owned()).collect();

    Ok(Exercise {
        id: id.to_owned(),
        exercise_type: ExerciseType::Distractor,
        difficulty,
        task,
        context: String::new(),
        available_tools: available,
        expected: ExpectedAnswer {
            tool_calls,
            expects_tool_call: true,
            answer_contains: None,
        },
        target_servers: servers,
        forbidden_tools: vec![],
    })
}

#[allow(clippy::unnecessary_wraps)] // Consistent return type with other generators
fn generate_no_tool(
    id: &str,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    let distractor_count = match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Medium => 3,
        Difficulty::Hard => 5.min(all_tools.len()),
    };

    let mut indices: Vec<usize> = (0..all_tools.len()).collect();
    indices.shuffle(rng);
    let distractors: Vec<ToolInfo> = indices[..distractor_count.min(all_tools.len())]
        .iter()
        .map(|&i| all_tools[i].1.clone())
        .collect();

    let context_hint = match difficulty {
        Difficulty::Easy => "The answer is: 42.",
        Difficulty::Medium => "Based on the data provided, the result can be computed directly.",
        Difficulty::Hard => {
            "The context contains all necessary information, though some tools \
             might seem relevant. Think carefully before calling any tool."
        }
    };

    let task = format!(
        "Answer the following question using ONLY the context provided. \
         Do NOT call any tool unless absolutely necessary.\n\
         Context: {context_hint}\n\
         Question: What is the answer?"
    );

    Ok(Exercise {
        id: id.to_owned(),
        exercise_type: ExerciseType::NoToolNeeded,
        difficulty,
        task,
        context: context_hint.to_owned(),
        available_tools: distractors,
        expected: ExpectedAnswer {
            tool_calls: vec![],
            expects_tool_call: false,
            answer_contains: Some("42".to_owned()),
        },
        target_servers: vec![],
        forbidden_tools: vec![],
    })
}

fn generate_cross_server(
    id: &str,
    difficulty: Difficulty,
    all_tools: &[(&str, &ToolInfo)],
    server_names: &[&str],
    rng: &mut impl Rng,
) -> Result<Exercise, String> {
    if server_names.len() < 2 {
        return Err("need at least 2 servers for cross-server exercises".into());
    }

    // Pick 2–3 servers based on difficulty.
    let server_count = match difficulty {
        Difficulty::Easy | Difficulty::Medium => 2,
        Difficulty::Hard => 3.min(server_names.len()),
    };

    let mut srv_indices: Vec<usize> = (0..server_names.len()).collect();
    srv_indices.shuffle(rng);
    let selected_servers: Vec<&str> = srv_indices[..server_count]
        .iter()
        .map(|&i| server_names[i])
        .collect();

    // Pick one tool per selected server.
    let mut tool_calls = Vec::new();
    let mut available = Vec::new();

    for &srv in &selected_servers {
        let server_tools: Vec<_> = all_tools.iter().filter(|(s, _)| *s == srv).collect();
        if server_tools.is_empty() {
            continue;
        }
        let idx = rng.gen_range(0..server_tools.len());
        let (server, tool) = server_tools[idx];
        tool_calls.push(ExpectedToolCall {
            tool_name: tool.name.clone(),
            server_name: (*server).to_owned(),
            expected_params: extract_required_params(&tool.input_schema),
        });
        available.push((*tool).clone());
    }

    let tool_list: Vec<String> = tool_calls
        .iter()
        .map(|tc| format!("'{}' from server '{}'", tc.tool_name, tc.server_name))
        .collect();

    let task = format!(
        "Complete this task using tools from multiple servers: {}. \
         Coordinate the results from each tool.",
        tool_list.join(", then ")
    );

    Ok(Exercise {
        id: id.to_owned(),
        exercise_type: ExerciseType::CrossServer,
        difficulty,
        task,
        context: String::new(),
        available_tools: available,
        expected: ExpectedAnswer {
            tool_calls,
            expects_tool_call: true,
            answer_contains: None,
        },
        target_servers: selected_servers.iter().map(|s| (*s).to_owned()).collect(),
        forbidden_tools: vec![],
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Distribute a count as evenly as possible across N buckets.
fn distribute_count(total: u32, buckets: usize) -> Vec<u32> {
    if buckets == 0 {
        return vec![];
    }
    let bucket_count = u32::try_from(buckets).unwrap_or(u32::MAX);
    let base = total / bucket_count;
    let remainder = total % bucket_count;
    let mut result = vec![base; buckets];
    for item in result.iter_mut().take(remainder as usize) {
        *item += 1;
    }
    result
}

/// Extract required parameter names from a JSON Schema `inputSchema`.
fn extract_required_params(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Check if a JSON Schema has `properties`.
fn has_properties(schema: &Value) -> bool {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|obj| !obj.is_empty())
}

fn exercise_type_slug(t: ExerciseType) -> &'static str {
    match t {
        ExerciseType::ToolSelection => "sel",
        ExerciseType::ParameterFilling => "param",
        ExerciseType::MultiStepChain => "chain",
        ExerciseType::ErrorRecovery => "err",
        ExerciseType::Distractor => "dist",
        ExerciseType::NoToolNeeded => "notool",
        ExerciseType::CrossServer => "cross",
    }
}

fn difficulty_slug(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Easy => "easy",
        Difficulty::Medium => "med",
        Difficulty::Hard => "hard",
    }
}

/// Serialize exercises to JSON for mcp-agent-gym scenarios.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn write_manifest(exercises: &[Exercise], path: &std::path::Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(exercises)
        .map_err(|e| format!("failed to serialize exercises: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("failed to write manifest: {e}"))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(
            "server_a".to_owned(),
            vec![
                ToolInfo {
                    name: "get_weather".to_owned(),
                    description: Some("Get weather for a location".to_owned()),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"]
                    }),
                },
                ToolInfo {
                    name: "search_docs".to_owned(),
                    description: Some("Search documentation".to_owned()),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"},
                            "limit": {"type": "integer"}
                        },
                        "required": ["query"]
                    }),
                },
            ],
        );
        reg.register(
            "server_b".to_owned(),
            vec![
                ToolInfo {
                    name: "run_query".to_owned(),
                    description: Some("Run a database query".to_owned()),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "sql": {"type": "string"}
                        },
                        "required": ["sql"]
                    }),
                },
                ToolInfo {
                    name: "list_tables".to_owned(),
                    description: Some("List database tables".to_owned()),
                    input_schema: serde_json::json!({"type": "object"}),
                },
            ],
        );
        reg
    }

    #[test]
    fn generates_correct_count() {
        let reg = test_registry();
        let config = GeneratorConfig {
            types: vec![ExerciseType::ToolSelection],
            difficulties: vec![Difficulty::Easy],
            count: 10,
            seed: 42,
        };
        let exercises = generate(&reg, &config).expect("should generate");
        assert_eq!(exercises.len(), 10);
    }

    #[test]
    fn deterministic_with_same_seed() {
        let reg = test_registry();
        let config = GeneratorConfig {
            types: vec![ExerciseType::ToolSelection],
            difficulties: vec![Difficulty::Easy, Difficulty::Medium],
            count: 6,
            seed: 123,
        };
        let a = generate(&reg, &config).expect("gen a");
        let b = generate(&reg, &config).expect("gen b");
        assert_eq!(a.len(), b.len());
        for (ea, eb) in a.iter().zip(b.iter()) {
            assert_eq!(ea.id, eb.id);
            assert_eq!(ea.task, eb.task);
        }
    }

    #[test]
    fn all_exercise_types() {
        let reg = test_registry();
        let config = GeneratorConfig {
            types: vec![
                ExerciseType::ToolSelection,
                ExerciseType::ParameterFilling,
                ExerciseType::MultiStepChain,
                ExerciseType::ErrorRecovery,
                ExerciseType::Distractor,
                ExerciseType::NoToolNeeded,
                ExerciseType::CrossServer,
            ],
            difficulties: vec![Difficulty::Easy],
            count: 7,
            seed: 99,
        };
        let exercises = generate(&reg, &config).expect("should generate");
        assert_eq!(exercises.len(), 7);
    }

    #[test]
    fn no_tool_needed_expects_no_call() {
        let reg = test_registry();
        let config = GeneratorConfig {
            types: vec![ExerciseType::NoToolNeeded],
            difficulties: vec![Difficulty::Easy],
            count: 1,
            seed: 42,
        };
        let exercises = generate(&reg, &config).expect("gen");
        assert!(!exercises[0].expected.expects_tool_call);
        assert!(exercises[0].expected.tool_calls.is_empty());
    }

    #[test]
    fn cross_server_uses_multiple_servers() {
        let reg = test_registry();
        let config = GeneratorConfig {
            types: vec![ExerciseType::CrossServer],
            difficulties: vec![Difficulty::Easy],
            count: 1,
            seed: 42,
        };
        let exercises = generate(&reg, &config).expect("gen");
        assert!(exercises[0].target_servers.len() >= 2);
    }

    #[test]
    fn empty_registry_errors() {
        let reg = ToolRegistry::new();
        let config = GeneratorConfig {
            types: vec![ExerciseType::ToolSelection],
            difficulties: vec![Difficulty::Easy],
            count: 1,
            seed: 42,
        };
        assert!(generate(&reg, &config).is_err());
    }

    #[test]
    fn distribute_count_even() {
        assert_eq!(distribute_count(6, 3), vec![2, 2, 2]);
    }

    #[test]
    fn distribute_count_remainder() {
        assert_eq!(distribute_count(7, 3), vec![3, 2, 2]);
    }

    #[test]
    fn extract_params_from_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"a": {}, "b": {}},
            "required": ["a"]
        });
        let params = extract_required_params(&schema);
        assert_eq!(params, vec!["a"]);
    }
}
