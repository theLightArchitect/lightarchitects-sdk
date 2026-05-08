//! Agent loop — reads user messages, calls LLM, dispatches tools, emits events.
//!
//! Uses the gateway's [`LlmClient`] and `core_tools` for execution.
//! Tool dispatch follows the same `### TOOL_CALL` / `### FINAL_OUTPUT`
//! convention as [`arena::agent_loop`].

use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::llm::LlmClient;
use crate::config::GatewayConfig;
use crate::core_tools;
use crate::error::GatewayError;

use super::protocol::{AgentEvent, ControlMessage, TerminationReason};

/// Maximum iterations per agent turn (prevents runaway).
const MAX_ITERATIONS: u32 = 10;

/// Timeout for a single LLM generation.
const LLM_TIMEOUT: Duration = Duration::from_secs(180);

/// Tool descriptions injected into the system prompt.
const TOOL_DESCRIPTIONS: &str = "\
## Available Tools

You can call tools by including a TOOL_CALL block in your response. Format:

### TOOL_CALL
```json
{\"tool\": \"<tool_name>\", \"args\": {<arguments>}}
```

After the tool executes, the result will be appended and you continue.
When you have enough information, output your final answer as:

### FINAL_OUTPUT
(your complete response here)

### Tools:

1. **bash** — Execute a shell command.
   Args: {\"command\": \"echo hello\", \"timeout_ms\": 120000, \"cwd\": \"/optional/path\"}
   Returns: exit_code + combined stdout/stderr.

2. **read** — Read a file with optional line-range selection.
   Args: {\"path\": \"/workspace/src/main.rs\", \"offset\": 1, \"limit\": 50}
   Returns: File contents with line numbers.

3. **write** — Create or overwrite a file atomically.
   Args: {\"path\": \"/workspace/src/lib.rs\", \"content\": \"pub fn hello() {}\"}
   Returns: bytes_written.

4. **edit** — Replace a string in a file.
   Args: {\"path\": \"/workspace/src/main.rs\", \"old_string\": \"fn old()\", \"new_string\": \"fn new()\"}
   Returns: confirmation.

5. **search** — Search files with ripgrep.
   Args: {\"pattern\": \"TODO\", \"path\": \"/workspace\", \"glob\": \"*.rs\"}
   Returns: Matching lines with file:line info.

6. **glob** — Find files matching a pattern.
   Args: {\"pattern\": \"**/*.rs\", \"path\": \"/workspace\"}
   Returns: List of matching paths.

RULES:
- ALWAYS read a file before editing or writing to it.
- Use bash for directory listings, git operations, and build commands.
- Use FINAL_OUTPUT only when you have completed the user's request.
- If a tool fails, try an alternative approach or explain the error.
";

/// Coding agent runner.
pub struct AgentRunner {
    llm: Arc<LlmClient>,
    config: GatewayConfig,
    cwd: std::path::PathBuf,
    interrupt_flag: Arc<AtomicBool>,
}

impl AgentRunner {
    /// Initialise the runner: create LLM client and minimal config.
    ///
    /// # Errors
    ///
    /// Returns an error if `LlmClient::from_env` fails.
    pub fn new(cwd: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let llm = LlmClient::from_env()?;
        let mut config = GatewayConfig::default();
        // Allow the user's home directory so the agent can work anywhere
        // the user has permission.
        if let Ok(home) = std::env::var("HOME") {
            config.allowed_directories.push(home);
        }
        Ok(Self {
            llm,
            config,
            cwd: cwd.to_path_buf(),
            interrupt_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Create a runner with a pre-built LLM client (TTY mode with custom backend).
    pub fn with_llm(cwd: &Path, llm: Arc<LlmClient>) -> Self {
        let mut config = GatewayConfig::default();
        if let Ok(home) = std::env::var("HOME") {
            config.allowed_directories.push(home);
        }
        Self {
            llm,
            config,
            cwd: cwd.to_path_buf(),
            interrupt_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    // ── NDJSON loop (machine-facing) ──────────────────────────────────────────

    /// Run the NDJSON stdin → stdout loop.
    pub async fn run_ndjson_loop(&mut self) {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let msg: ControlMessage = match serde_json::from_str(&line) {
                Ok(m) => m,
                Err(e) => {
                    self.emit_ndjson(&AgentEvent::Error {
                        message: format!("parse error: {e}"),
                        recoverable: Some(true),
                    },
                    &mut stdout)
                    .await;
                    continue;
                }
            };

            match msg {
                ControlMessage::SendMessage { text } => {
                    self.interrupt_flag.store(false, Ordering::SeqCst);
                    self.run_turn(&text, &mut stdout, true).await;
                }
                ControlMessage::Interrupt => {
                    self.interrupt_flag.store(true, Ordering::SeqCst);
                    self.emit_ndjson(
                    &AgentEvent::Error {
                        message: "interrupted".to_owned(),
                        recoverable: Some(true),
                    },
                    &mut stdout)
                    .await;
                }
                ControlMessage::Steer { text } => {
                    self.emit_ndjson(
                    &AgentEvent::StatusUpdate {
                        text: format!("steer: {text}"),
                    },
                    &mut stdout)
                    .await;
                }
                ControlMessage::Ping => {
                    self.emit_ndjson(&AgentEvent::Heartbeat, &mut stdout).await;
                }
                _ => {
                    self.emit_ndjson(
                    &AgentEvent::Error {
                        message: "unsupported control message".to_owned(),
                        recoverable: Some(true),
                    },
                    &mut stdout)
                    .await;
                }
            }
        }
    }

    // ── Interactive loop (human-facing) ─────────────────────────────────────

    /// Run an interactive REPL on the terminal.
    pub async fn run_interactive_loop(&mut self) {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        let banner = format!(
            "Light Architects agent — cwd: {}\nType 'quit' or press Ctrl-D to exit.\n",
            self.cwd.display()
        );
        let _ = stdout.write_all(banner.as_bytes()).await;
        let _ = stdout.flush().await;

        loop {
            let _ = stdout.write_all(b"> ").await;
            let _ = stdout.flush().await;

            let Ok(Some(line)) = lines.next_line().await else { break };

            let input = line.trim();
            if input.is_empty() {
                continue;
            }
            if input.eq_ignore_ascii_case("quit")
                || input.eq_ignore_ascii_case("exit")
            {
                break;
            }

            self.interrupt_flag.store(false, Ordering::SeqCst);
            self.run_turn(input, &mut stdout, false).await;
        }
    }

    // ── Core turn logic ───────────────────────────────────────────────────────

    /// Run a single agent turn: user message → LLM → tools → final output.
    #[allow(clippy::too_many_lines)]
    async fn run_turn(
        &self,
        user_message: &str,
        sink: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
        ndjson: bool,
    ) {
        let mut conversation = format!(
            "You are a helpful coding assistant. You have access to tools.\n\n{TOOL_DESCRIPTIONS}\n\nUser: {user_message}\n\nAssistant:"
        );

        for iteration in 0..MAX_ITERATIONS {
            if self.interrupt_flag.load(Ordering::Relaxed) {
                self.emit(
                    &AgentEvent::Complete {
                        reason: TerminationReason::UserCancelled,
                    },
                    sink,
                    ndjson,
                )
                .await;
                return;
            }

            let response = match tokio::time::timeout(
                LLM_TIMEOUT,
                self.llm.generate(&conversation),
            )
            .await
            {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    self.emit(
                        &AgentEvent::Error {
                            message: format!("LLM error: {e}"),
                            recoverable: Some(false),
                        },
                        sink,
                        ndjson,
                    )
                    .await;
                    self.emit(
                        &AgentEvent::Complete {
                            reason: TerminationReason::Error {
                                message: format!("LLM error: {e}"),
                            },
                        },
                        sink,
                        ndjson,
                    )
                    .await;
                    return;
                }
                Err(_) => {
                    self.emit(
                        &AgentEvent::Error {
                            message: "LLM timeout".to_owned(),
                            recoverable: Some(true),
                        },
                        sink,
                        ndjson,
                    )
                    .await;
                    self.emit(
                        &AgentEvent::Complete {
                            reason: TerminationReason::Timeout,
                        },
                        sink,
                        ndjson,
                    )
                    .await;
                    return;
                }
            };

            // Stream text chunks to the sink as we parse
            if let Some(final_output) = extract_final_output(&response) {
                if !final_output.is_empty() {
                    self.emit(
                        &AgentEvent::Text {
                            chunk: final_output.clone(),
                        },
                        sink,
                        ndjson,
                    )
                    .await;
                }
                self.emit(
                    &AgentEvent::Complete {
                        reason: TerminationReason::Complete,
                    },
                    sink,
                    ndjson,
                )
                .await;
                return;
            }

            if let Some(tool_call) = extract_tool_call(&response) {
                let tool_id = format!("call_{iteration}");
                self.emit(
                    &AgentEvent::ToolStart {
                        name: tool_call.tool.clone(),
                        id: tool_id.clone(),
                        input: tool_call.args.clone(),
                    },
                    sink,
                    ndjson,
                )
                .await;

                let start = Instant::now();
                let result = execute_tool(&tool_call,
                    &self.config,
                    &self.cwd,
                )
                .await;
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

                let (success, result_text) = match result {
                    Ok(val) => {
                        let text = val["content"]
                            .get(0)
                            .and_then(|c| c["text"].as_str())
                            .unwrap_or("(empty result)")
                            .to_owned();
                        (true, text)
                    }
                    Err(e) => (false, e.to_string()),
                };

                self.emit(
                    &AgentEvent::ToolComplete {
                        id: tool_id,
                        success,
                        duration_ms,
                        result: Some(result_text.clone()),
                    },
                    sink,
                    ndjson,
                )
                .await;

                // Append tool result to conversation for next LLM call
                let prefix = response
                    .find("### TOOL_CALL")
                    .map_or(response.as_str(), |pos| &response[..pos]);
                let _ = write!(
                    conversation,
                    "\n\nAssistant: {prefix}\n\n\
                     ### TOOL_RESULT\n```\n{result_text}\n```\n\n\
                     Continue with your analysis. Use another tool or write ### FINAL_OUTPUT.\n"
                );
            } else {
                // No parseable tool call and no FINAL_OUTPUT
                let cleaned = strip_tool_blocks(&response);
                if cleaned.trim().is_empty() {
                    let _ = write!(
                        conversation,
                        "\n\nAssistant: {response}\n\n\
                         Your previous response contained a TOOL_CALL that could not be parsed. \
                         Use a single JSON object: {{\"tool\": \"name\", \"args\": {{...}}}}. \
                         Or write ### FINAL_OUTPUT.\n"
                    );
                    continue;
                }
                self.emit(
                    &AgentEvent::Text {
                        chunk: cleaned.clone(),
                    },
                    sink,
                    ndjson,
                )
                .await;
                self.emit(
                    &AgentEvent::Complete {
                        reason: TerminationReason::Complete,
                    },
                    sink,
                    ndjson,
                )
                .await;
                return;
            }
        }

        self.emit(
            &AgentEvent::Error {
                message: format!("Max iterations ({MAX_ITERATIONS}) reached"),
                recoverable: Some(true),
            },
            sink,
            ndjson,
        )
        .await;
        self.emit(
            &AgentEvent::Complete {
                reason: TerminationReason::MaxIterations,
            },
            sink,
            ndjson,
        )
        .await;
    }

    // ── Output helpers ────────────────────────────────────────────────────────

    async fn emit_ndjson(
        &self,
        ev: &AgentEvent,
        sink: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
    ) {
        let Ok(json) = serde_json::to_string(ev) else { return };
        let line = format!("{json}\n");
        let _ = sink.write_all(line.as_bytes()).await;
        let _ = sink.flush().await;
    }

    async fn emit(
        &self,
        ev: &AgentEvent,
        sink: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
        ndjson: bool,
    ) {
        if ndjson {
            self.emit_ndjson(ev, sink).await;
        } else {
            match ev {
                AgentEvent::Text { chunk } => {
                    let _ = sink.write_all(chunk.as_bytes()).await;
                    let _ = sink.flush().await;
                }
                AgentEvent::ToolStart { name, .. } => {
                    let msg = format!("\n[tool: {name}]\n");
                    let _ = sink.write_all(msg.as_bytes()).await;
                    let _ = sink.flush().await;
                }
                AgentEvent::ToolComplete { success, result, .. } => {
                    let status = if *success { "✓" } else { "✗" };
                    let result_str = result.as_deref().unwrap_or("(no result)");
                    let msg = format!("{status} {result_str}\n");
                    let _ = sink.write_all(msg.as_bytes()).await;
                    let _ = sink.flush().await;
                }
                AgentEvent::Error { message, .. } => {
                    let msg = format!("\nError: {message}\n");
                    let _ = sink.write_all(msg.as_bytes()).await;
                    let _ = sink.flush().await;
                }
                AgentEvent::Complete { .. } => {
                    let _ = sink.write_all(b"\n").await;
                    let _ = sink.flush().await;
                }
                _ => {}
            }
        }
    }
}

// ── Tool call parsing ───────────────────────────────────────────────────────

#[derive(Debug)]
struct ToolCall {
    tool: String,
    args: Value,
}

fn extract_tool_call(response: &str) -> Option<ToolCall> {
    let marker = "### TOOL_CALL";
    let start = response.find(marker)?;
    let after = &response[start + marker.len()..];

    let json_str = if let Some(code_start) = after.find("```") {
        let inner = &after[code_start + 3..];
        let inner = inner.strip_prefix("json").unwrap_or(inner).trim_start();
        let code_end = inner.find("```")?;
        &inner[..code_end]
    } else {
        let trimmed = after.trim();
        let brace_start = trimmed.find(['{', '['])?;
        let bracket = trimmed.as_bytes().get(brace_start)?;
        let closing = if *bracket == b'[' { ']' } else { '}' };
        let brace_end = trimmed.rfind(closing)?;
        &trimmed[brace_start..=brace_end]
    };

    let parsed: Value = serde_json::from_str(json_str.trim()).ok()?;
    let obj = if let Some(arr) = parsed.as_array() {
        arr.first()?.clone()
    } else {
        parsed
    };

    let tool = obj.get("tool")?.as_str()?.to_owned();
    let args = obj.get("args").cloned().unwrap_or(Value::Null);
    Some(ToolCall { tool, args })
}

fn extract_final_output(response: &str) -> Option<String> {
    let marker = "### FINAL_OUTPUT";
    let start = response.find(marker)?;
    let content = &response[start + marker.len()..];
    let end = content.find("\n### ").unwrap_or(content.len());
    let section = content[..end].trim().to_owned();
    if section.is_empty() {
        None
    } else {
        Some(section)
    }
}

fn strip_tool_blocks(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    while let Some(start) = remaining
        .find("### TOOL_CALL")
        .or_else(|| remaining.find("### TOOL_RESULT"))
    {
        result.push_str(&remaining[..start]);
        let after_marker = &remaining[start..];
        if let Some(code_start) = after_marker.find("```") {
            let after_code = &after_marker[code_start + 3..];
            if let Some(code_end) = after_code.find("```") {
                remaining = &after_code[code_end + 3..];
                continue;
            }
        }
        let line_end = after_marker.find('\n').unwrap_or(after_marker.len());
        remaining = &after_marker[line_end..];
    }

    result.push_str(remaining);
    result
}

// ── Tool execution ──────────────────────────────────────────────────────────

const ALLOWED_TOOLS: &[&str] = &[
    "bash", "read", "write", "edit", "search", "glob",
];

async fn execute_tool(
    call: &ToolCall,
    config: &GatewayConfig,
    cwd: &Path,
) -> Result<Value, GatewayError> {
    if !ALLOWED_TOOLS.contains(&call.tool.as_str()) {
        return Err(GatewayError::UnknownTool(call.tool.clone()));
    }

    // Inject cwd into args if not already present
    let mut params = call.args.clone();
    if call.tool == "bash" {
        if params.get("cwd").is_none() {
            params["cwd"] = json!(cwd.to_string_lossy().to_string());
        }
    } else if matches!(call.tool.as_str(), "read" | "write" | "edit" | "search" | "glob")
        && params.get("path").is_none()
    {
        // Some tools use 'path' as the directory/file target
        // We leave it absent so the tool can report missing param
    }

    match call.tool.as_str() {
        "bash" => core_tools::bash::run(params).await,
        "read" => core_tools::read::run(params, config),
        "write" => core_tools::write::run(params, config),
        "edit" => core_tools::edit::run(params, config),
        "search" => core_tools::search::run(params, config).await,
        "glob" => core_tools::glob::run(params, config).await,
        _ => unreachable!(),
    }
}
