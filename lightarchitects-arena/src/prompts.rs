//! Templatized prompt system for different training objectives.
//!
//! Generates prompts that work for any tool surface by auto-inserting
//! discovered tool schemas. Supports SFT, RL, and DPO prompt variants
//! with thinking-model awareness (handles `reasoning_content` field).

use lightarchitects_core::action::ToolInfo;
use serde::{Deserialize, Serialize};

use crate::config::OutputFormat;
use crate::exercises::Exercise;

/// A fully assembled prompt ready to send to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledPrompt {
    /// System message (tool schemas + instructions).
    pub system: String,
    /// User message (exercise task + context).
    pub user: String,
    /// Training objective this prompt is designed for.
    pub objective: OutputFormat,
}

/// Prompt configuration options.
#[derive(Debug, Clone)]
pub struct PromptConfig {
    /// Training objective (affects prompt framing).
    pub objective: OutputFormat,
    /// Maximum tokens hint for the model.
    pub max_tokens: u32,
    /// Whether the model supports thinking/reasoning mode.
    pub thinking_model: bool,
}

/// Build the system message containing tool schemas and instructions.
///
/// The system message is the same regardless of training objective —
/// it tells the model what tools are available and how to call them.
fn build_system_message(tools: &[ToolInfo]) -> String {
    let mut parts = Vec::with_capacity(tools.len().saturating_add(2));

    parts.push(
        "You are an AI agent with access to the following tools. \
         When you need to use a tool, respond with a JSON object containing \
         \"tool\" (the tool name) and \"params\" (the parameters)."
            .to_owned(),
    );

    parts.push(String::new()); // blank line

    for tool in tools {
        let desc = tool
            .description
            .as_deref()
            .unwrap_or("No description available");

        parts.push(format!(
            "### {}\n{}\nInput schema:\n```json\n{}\n```",
            tool.name,
            desc,
            serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default()
        ));
    }

    parts.push(String::new());
    parts.push(
        "Rules:\n\
         - Use ONLY tools from the list above\n\
         - Respond with exactly one JSON tool call per step\n\
         - Include all required parameters\n\
         - If no tool is needed, respond with your answer directly"
            .to_owned(),
    );

    parts.join("\n\n")
}

/// Build the user message from an exercise.
fn build_user_message(exercise: &Exercise, config: &PromptConfig) -> String {
    let mut parts = Vec::new();

    // Objective-specific framing.
    match config.objective {
        OutputFormat::Sft => {
            parts.push("Complete the following task correctly.".to_owned());
        }
        OutputFormat::Rl => {
            parts.push(
                "Complete the following task. You may explore different approaches.".to_owned(),
            );
        }
        OutputFormat::Dpo => {
            parts.push("Complete the following task as accurately as possible.".to_owned());
        }
    }

    if !exercise.context.is_empty() {
        parts.push(format!("Context: {}", exercise.context));
    }

    parts.push(format!("Task: {}", exercise.task));

    if config.thinking_model {
        parts.push(
            "Think step-by-step before responding. \
             Your reasoning will be recorded."
                .to_owned(),
        );
    }

    parts.join("\n\n")
}

/// Assemble a complete prompt for an exercise.
///
/// Combines tool schemas into the system message and the exercise task
/// into the user message, with framing appropriate for the training
/// objective.
#[must_use]
pub fn assemble(exercise: &Exercise, config: &PromptConfig) -> AssembledPrompt {
    let system = build_system_message(&exercise.available_tools);
    let user = build_user_message(exercise, config);

    AssembledPrompt {
        system,
        user,
        objective: config.objective,
    }
}

/// Format a prompt as `ChatML` messages for API consumption.
///
/// Returns a vector of `{"role": "...", "content": "..."}` objects
/// suitable for OpenAI-compatible `/v1/chat/completions` endpoints.
#[must_use]
pub fn to_chat_messages(prompt: &AssembledPrompt) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "role": "system",
            "content": prompt.system
        }),
        serde_json::json!({
            "role": "user",
            "content": prompt.user
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tools() -> Vec<ToolInfo> {
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
                name: "search".to_owned(),
                description: Some("Search documents".to_owned()),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    },
                    "required": ["query"]
                }),
            },
        ]
    }

    fn test_exercise() -> Exercise {
        use crate::config::{Difficulty, ExerciseType};
        use crate::exercises::{ExpectedAnswer, ExpectedToolCall};

        Exercise {
            id: "test-1".to_owned(),
            exercise_type: ExerciseType::ToolSelection,
            difficulty: Difficulty::Easy,
            task: "Get the weather in London".to_owned(),
            context: String::new(),
            available_tools: test_tools(),
            expected: ExpectedAnswer {
                tool_calls: vec![ExpectedToolCall {
                    tool_name: "get_weather".to_owned(),
                    server_name: "test".to_owned(),
                    expected_params: vec!["location".to_owned()],
                }],
                expects_tool_call: true,
                answer_contains: None,
            },
            target_servers: vec!["test".to_owned()],
            forbidden_tools: vec![],
        }
    }

    #[test]
    fn system_message_includes_tools() {
        let msg = build_system_message(&test_tools());
        assert!(msg.contains("get_weather"));
        assert!(msg.contains("search"));
        assert!(msg.contains("Input schema:"));
        assert!(msg.contains("location"));
    }

    #[test]
    fn sft_prompt_framing() {
        let exercise = test_exercise();
        let config = PromptConfig {
            objective: OutputFormat::Sft,
            max_tokens: 2048,
            thinking_model: false,
        };
        let prompt = assemble(&exercise, &config);
        assert!(prompt.user.contains("correctly"));
        assert!(!prompt.user.contains("Think step-by-step"));
    }

    #[test]
    fn rl_prompt_framing() {
        let exercise = test_exercise();
        let config = PromptConfig {
            objective: OutputFormat::Rl,
            max_tokens: 2048,
            thinking_model: false,
        };
        let prompt = assemble(&exercise, &config);
        assert!(prompt.user.contains("explore"));
    }

    #[test]
    fn thinking_model_adds_instruction() {
        let exercise = test_exercise();
        let config = PromptConfig {
            objective: OutputFormat::Sft,
            max_tokens: 2048,
            thinking_model: true,
        };
        let prompt = assemble(&exercise, &config);
        assert!(prompt.user.contains("Think step-by-step"));
    }

    #[test]
    fn chat_messages_format() {
        let exercise = test_exercise();
        let config = PromptConfig {
            objective: OutputFormat::Sft,
            max_tokens: 2048,
            thinking_model: false,
        };
        let prompt = assemble(&exercise, &config);
        let messages = to_chat_messages(&prompt);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn context_included_when_present() {
        let mut exercise = test_exercise();
        exercise.context = "Prior data: temperature was 20C".to_owned();
        let config = PromptConfig {
            objective: OutputFormat::Sft,
            max_tokens: 2048,
            thinking_model: false,
        };
        let prompt = assemble(&exercise, &config);
        assert!(prompt.user.contains("Prior data: temperature was 20C"));
    }
}
