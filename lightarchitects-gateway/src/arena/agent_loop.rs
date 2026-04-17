//! `ReAct` agent loop — gives siblings tool-calling agency.
//!
//! Each heartbeat runs a multi-turn loop: the LLM generates text, and if it
//! includes a `### TOOL_CALL` block, the tool is executed and the result is
//! appended to the conversation. The loop continues until `### FINAL_OUTPUT`
//! or the iteration limit is reached.
//!
//! Phase 1 tools: `fetch_paper`, `search_papers`.
//! Phase 2 tools: `write_file` (vault persistence).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::compat::JsonRpcResponseExt;
use super::llm::LlmClient;
use super::mcp_pool::McpPool;
use lightarchitects::core::jsonrpc::JsonRpcRequest;
use lightarchitects::core::paths;

/// Maximum iterations per agent loop (prevents runaway).
const MAX_ITERATIONS: u32 = 5;

/// HTTP timeout for paper fetches.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Tool descriptions injected into the system prompt.
pub const TOOL_DESCRIPTIONS: &str = "\
## Available Tools

You can call tools by including a TOOL_CALL block in your response. Format:

### TOOL_CALL
```json
{\"tool\": \"<tool_name>\", \"args\": {<arguments>}}
```

After the tool executes, the result will be appended and you continue.
When you have enough information, output your final answer as:

### FINAL_OUTPUT
(your complete output here)

### Tools:

1. **fetch_paper** — Fetch a paper's abstract and metadata from arXiv (quick overview).
   Args: {\"id\": \"2503.04302\"} (arXiv ID without the 'arXiv:' prefix)
   Returns: Title, authors, abstract, categories.

2. **read_paper** — Fetch the FULL paper text from arXiv HTML (deep read, use before analysis).
   Args: {\"id\": \"2503.04302\"}
   Returns: Full paper content including introduction, methodology, results, conclusions.
   This gives you the actual paper, not just the abstract. Use this to write substantive analysis.

3. **search_papers** — Search HuggingFace for papers matching a query.
   Args: {\"query\": \"multi-agent collaboration LLM\", \"limit\": 5}
   Returns: List of matching papers with titles, authors, and summaries.

4. **query_helix** — Query the SOUL knowledge graph for prior work, decisions, and context.
   Args: {\"sibling\": \"eva\", \"strand\": \"tactical\", \"min_significance\": 7.0, \"limit\": 5}
   All args optional. Use this to:
   - Find what YOU previously wrote about a topic: {\"sibling\": \"YOUR_NAME\"}
   - Find what OTHER siblings wrote: {\"sibling\": \"corso\"}
   - Find high-significance decisions: {\"min_significance\": 8.0}
   - Ground your analysis in real helix data instead of making assumptions.
   RECOMMENDED: Query helix BEFORE analyzing a paper to check if someone already covered the topic.

5. **read_file** — Read a file from the shared workspace or helix vault (read-only).
   Args: {\"path\": \"shared/thinktank/some-analysis.md\"} or {\"path\": \"helix/eva/identity.md\"}
   Use this to read specific vault files, prior analyses, or sibling identities.

6. **write_file** — Write content to the SOUL vault for permanent persistence.
   Args: {\"path\": \"shared/thinktank/my-analysis-eva-2026-03-22.md\", \"content\": \"...\"}
   Allowed directories:
   - shared/thinktank/          (research discussions)
   - shared/research/summaries/ (paper summaries)
   - shared/devotionals/reflections/ (devotional reflections)
   - {your-sibling}/journal/    (personal journal entries, e.g. eva/journal/2026-03-22-morning.md)
   Creates parent directories automatically. ALWAYS use this to persist your analysis.
   Without write_file, your output is lost when the next heartbeat runs.

RULES:
- BEFORE writing your analysis, use query_helix or read_file to gather context from the vault.
  Check if the topic was already covered or if prior work exists that you should build on.
- Use fetch_paper to get the full abstract of any paper you want to analyze in depth.
- ALWAYS use write_file to persist your analysis before FINAL_OUTPUT.
- Only use FINAL_OUTPUT when you have saved your work via write_file.
- query_helix and read_file are READ-ONLY — they cannot modify any data.
";

/// Result of running the agent loop.
pub struct AgentResult {
    /// The final output text (from `FINAL_OUTPUT` section).
    pub output: String,
    /// Number of tool calls made.
    pub tool_calls: u32,
}

/// Run the agent loop: prompt -> tool calls -> final output.
///
/// `mcp_pool` enables helix read tools (`query_helix`, `read_file`).
/// `data_dir` is the `Arena` data root for resolving file paths.
#[tracing::instrument(skip(llm, initial_prompt, mcp_pool, data_dir))]
pub async fn run(
    llm: &LlmClient,
    initial_prompt: &str,
    mcp_pool: Option<&McpPool>,
    data_dir: Option<&Path>,
) -> Result<AgentResult, String> {
    let mut conversation = initial_prompt.to_owned();
    let mut tool_calls: u32 = 0;

    for iteration in 0..MAX_ITERATIONS {
        let response = llm.generate(&conversation).await?;

        // Check for FINAL_OUTPUT
        if let Some(final_output) = extract_final_output(&response) {
            return Ok(AgentResult {
                output: final_output,
                tool_calls,
            });
        }

        // Check for TOOL_CALL
        if let Some(tool_call) = extract_tool_call(&response) {
            tracing::info!(
                tool = %tool_call.tool,
                iteration,
                "Agent tool call"
            );
            let tool_result = execute_tool(&tool_call, mcp_pool, data_dir).await;
            tool_calls = tool_calls.saturating_add(1);

            // Append the tool result to the conversation
            let prefix = response
                .find("### TOOL_CALL")
                .map_or(response.as_str(), |pos| &response[..pos]);
            let _ = write!(
                conversation,
                "\n\nAssistant: {prefix}\n\n\
                 ### TOOL_RESULT\n```\n{tool_result}\n```\n\n\
                 Continue with your analysis. Use another tool or write ### FINAL_OUTPUT.\n"
            );
        } else {
            // No parseable tool call and no FINAL_OUTPUT.
            // Strip any raw TOOL_CALL/TOOL_RESULT blocks that leaked through
            // (e.g., malformed JSON the parser couldn't handle).
            let cleaned = strip_tool_blocks(&response);
            if cleaned.trim().is_empty() {
                tracing::warn!(
                    iteration,
                    "Agent produced only unparseable tool blocks — no usable output"
                );
                // Give it another turn with guidance
                let _ = write!(
                    conversation,
                    "\n\nAssistant: {response}\n\n\
                     Your previous response contained a TOOL_CALL that could not be parsed. \
                     Use a single JSON object (not an array): \
                     {{\"tool\": \"name\", \"args\": {{...}}}}. \
                     Or write ### FINAL_OUTPUT with your analysis.\n"
                );
                continue;
            }
            return Ok(AgentResult {
                output: cleaned,
                tool_calls,
            });
        }
    }

    Err(format!(
        "Agent loop hit max iterations ({MAX_ITERATIONS}) without FINAL_OUTPUT"
    ))
}

// ── Tool Call Parsing ──────────────────────────────────────────────────

#[derive(Debug)]
struct ToolCall {
    tool: String,
    args: serde_json::Value,
}

/// Extract the first tool call from a response.
///
/// Handles both single-object and array formats:
/// - `{"tool": "fetch_paper", "args": {...}}`
/// - `[{"tool": "fetch_paper", "args": {...}}, {"tool": "query_helix", "args": {...}}]`
///
/// When an array is provided, only the first element is extracted. The remaining
/// calls are logged but not queued (the agent can re-request on the next iteration).
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
        // Fallback: find outermost braces or brackets
        let trimmed = after.trim();
        let brace_start = trimmed.find(['{', '['])?;
        let bracket = trimmed.as_bytes().get(brace_start)?;
        let closing = if *bracket == b'[' { ']' } else { '}' };
        let brace_end = trimmed.rfind(closing)?;
        &trimmed[brace_start..=brace_end]
    };

    let parsed: serde_json::Value = serde_json::from_str(json_str.trim()).ok()?;

    // Handle array format: take first element, log the rest
    let obj = if let Some(arr) = parsed.as_array() {
        if arr.len() > 1 {
            tracing::info!(
                count = arr.len(),
                "Model emitted array of tool calls — executing first, rest will retry"
            );
        }
        arr.first()?.clone()
    } else {
        parsed
    };

    let tool = obj.get("tool")?.as_str()?.to_owned();
    let args = obj.get("args").cloned().unwrap_or(serde_json::Value::Null);

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

/// Strip raw `### TOOL_CALL` and `### TOOL_RESULT` blocks from a response.
///
/// Used when the agent loop falls through without a parseable tool call
/// or `FINAL_OUTPUT` — prevents raw JSON blocks from leaking into Discord/helix.
fn strip_tool_blocks(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    while let Some(start) = remaining
        .find("### TOOL_CALL")
        .or_else(|| remaining.find("### TOOL_RESULT"))
    {
        result.push_str(&remaining[..start]);

        // Skip past the block: find the closing ``` after the opening ```
        let after_marker = &remaining[start..];
        if let Some(code_start) = after_marker.find("```") {
            let after_code = &after_marker[code_start + 3..];
            if let Some(code_end) = after_code.find("```") {
                remaining = &after_code[code_end + 3..];
                continue;
            }
        }
        // No code block found — skip to end of line
        let line_end = after_marker.find('\n').unwrap_or(after_marker.len());
        remaining = &after_marker[line_end..];
    }

    result.push_str(remaining);
    result
}

// ── Tool Execution ─────────────────────────────────────────────────────

/// Exhaustive list of tool names the agent loop will dispatch.
///
/// Serves as an auditable allowlist — any tool name not in this list is rejected
/// before reaching the dispatch match, regardless of what the LLM requests.
const ALLOWED_AGENT_TOOLS: &[&str] = &[
    "fetch_paper",
    "read_paper",
    "search_papers",
    "query_helix",
    "read_file",
    "write_file",
    "write",
];

#[tracing::instrument(skip(mcp_pool, data_dir), fields(tool = %call.tool))]
async fn execute_tool(
    call: &ToolCall,
    mcp_pool: Option<&McpPool>,
    data_dir: Option<&Path>,
) -> String {
    match call.tool.as_str() {
        "fetch_paper" => fetch_paper(&call.args).await,
        "read_paper" => read_paper_full(&call.args).await,
        "search_papers" => search_papers(&call.args).await,
        "query_helix" => query_helix(&call.args, mcp_pool).await,
        "read_file" => read_file_tool(&call.args, data_dir),
        "write_file" | "write" => write_file_tool(&call.args, data_dir),
        _ => format!(
            "Unknown tool '{}'. Allowed tools: {}",
            call.tool,
            ALLOWED_AGENT_TOOLS.join(", ")
        ),
    }
}

/// Fetch a paper from arXiv by ID.
async fn fetch_paper(args: &serde_json::Value) -> String {
    let Some(id) = args.get("id").and_then(serde_json::Value::as_str) else {
        return "Error: missing 'id' arg. Usage: {\"id\": \"2503.04302\"}".into();
    };

    let clean_id: String = id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.')
        .take(20)
        .collect();

    let url = format!("http://export.arxiv.org/api/query?id_list={clean_id}&max_results=1");

    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .unwrap_or_default();

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            parse_arxiv_response(&body, &clean_id)
        }
        Ok(resp) => format!("arXiv returned status {}", resp.status()),
        Err(e) => format!("Failed to fetch paper: {e}"),
    }
}

fn parse_arxiv_response(xml: &str, id: &str) -> String {
    let title = extract_xml_tag(xml, "title")
        .unwrap_or_else(|| "Unknown title".into())
        .replace('\n', " ");
    let summary = extract_xml_tag(xml, "summary")
        .unwrap_or_else(|| "No abstract available".into())
        .replace('\n', " ");
    let authors: Vec<String> = xml
        .split("<author>")
        .skip(1)
        .filter_map(|chunk| extract_xml_tag(chunk, "name"))
        .collect();
    let author_str = if authors.is_empty() {
        "Unknown".into()
    } else {
        authors.join(", ")
    };

    format!("arXiv:{id}\nTitle: {title}\nAuthors: {author_str}\n\nAbstract:\n{summary}")
}

pub fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?;
    let after_open = &xml[start..];
    let content_start = after_open.find('>')? + 1;
    let content = &after_open[content_start..];
    let end = content.find(&close)?;
    Some(content[..end].trim().to_owned())
}

/// Maximum chars returned from full paper read.
const READ_PAPER_MAX_CHARS: usize = 12_000;

/// Fetch the full paper text from arXiv HTML version.
///
/// arXiv provides HTML versions at `https://arxiv.org/html/{id}`. This gives
/// the agent the actual paper content (intro, methodology, results, conclusions)
/// instead of just the abstract. Falls back to abstract-only if HTML unavailable.
async fn read_paper_full(args: &serde_json::Value) -> String {
    let Some(id) = args.get("id").and_then(serde_json::Value::as_str) else {
        return "Error: missing 'id' arg. Usage: {\"id\": \"2503.04302\"}".into();
    };

    let clean_id: String = id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == 'v')
        .take(20)
        .collect();

    // Try HTML version first (most recent papers have it)
    let html_url = format!("https://arxiv.org/html/{clean_id}");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    match client
        .get(&html_url)
        .header("User-Agent", "lightarchitects-arena/1.0")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let html = resp.text().await.unwrap_or_default();
            let text = html_to_text(&html);
            if text.len() > 500 {
                let truncated: String = text.chars().take(READ_PAPER_MAX_CHARS).collect();
                return format!(
                    "Full paper text (arXiv:{clean_id}, {len} chars):\n\n{truncated}",
                    len = truncated.len()
                );
            }
            // HTML was too short — likely an error page, fall back to abstract
            tracing::info!(id = %clean_id, "HTML too short, falling back to abstract");
        }
        Ok(resp) => {
            tracing::info!(id = %clean_id, status = %resp.status(), "HTML not available, falling back to abstract");
        }
        Err(e) => {
            tracing::info!(id = %clean_id, error = %e, "HTML fetch failed, falling back to abstract");
        }
    }

    // Fallback: fetch abstract via arXiv API
    let result = fetch_paper(args).await;
    format!("(Full paper HTML not available — abstract only)\n\n{result}")
}

/// Strip HTML tags and extract readable text from arXiv HTML papers.
///
/// Preserves section headings, paragraph breaks, and list items.
/// Strips scripts, styles, navigation, and metadata.
fn html_to_text(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut last_was_space = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_name.clear();
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
                let lower = tag_name.to_lowercase();

                // Track script/style blocks
                if lower.starts_with("script") {
                    in_script = true;
                } else if lower.starts_with("/script") {
                    in_script = false;
                } else if lower.starts_with("style") {
                    in_style = true;
                } else if lower.starts_with("/style") {
                    in_style = false;
                }

                // Insert line breaks for block elements
                if lower.starts_with("p")
                    || lower.starts_with("/p")
                    || lower.starts_with("br")
                    || lower.starts_with("h1")
                    || lower.starts_with("h2")
                    || lower.starts_with("h3")
                    || lower.starts_with("h4")
                    || lower.starts_with("/h")
                    || lower.starts_with("li")
                    || lower.starts_with("div")
                {
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                    // Add heading marker
                    if lower.starts_with("h1") || lower.starts_with("h2") || lower.starts_with("h3")
                    {
                        result.push_str("## ");
                    }
                    last_was_space = true;
                }
            } else {
                tag_name.push(ch);
            }
            continue;
        }

        // Skip content inside script/style blocks
        if in_script || in_style {
            continue;
        }

        // Collapse whitespace
        if ch.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            // Decode common HTML entities
            result.push(ch);
            last_was_space = false;
        }
    }

    // Decode HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Search `HuggingFace` papers API.
async fn search_papers(args: &serde_json::Value) -> String {
    let query = args
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("AI research");
    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(5)
        .min(10);

    let encoded_query = query.replace(' ', "+");
    let url = format!("https://huggingface.co/api/papers?query={encoded_query}&limit={limit}");

    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .unwrap_or_default();

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let papers: Vec<serde_json::Value> = resp.json().await.unwrap_or_default();
            if papers.is_empty() {
                return format!("No papers found for query: {query}");
            }
            let mut result = format!("Found {} papers for \"{query}\":\n\n", papers.len());
            for (i, paper) in papers.iter().enumerate() {
                let title = paper
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Untitled");
                let pid = paper
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let summary = paper
                    .get("summary")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("No summary");
                let preview: String = summary.chars().take(200).collect();
                let _ = write!(
                    result,
                    "{}. **{title}** ({pid})\n   {preview}...\n\n",
                    i + 1
                );
            }
            result
        }
        Ok(resp) => format!("HuggingFace API returned status {}", resp.status()),
        Err(e) => format!("Search failed: {e}"),
    }
}

// ── Helix Read Tools ──────────────────────────────────────────────────

/// Maximum bytes returned from `read_file` tool.
const READ_FILE_MAX_BYTES: usize = 4096;

/// Query the SOUL helix knowledge graph via MCP pool.
async fn query_helix(args: &serde_json::Value, mcp_pool: Option<&McpPool>) -> String {
    let Some(pool) = mcp_pool else {
        return "Error: helix query unavailable (MCP pool not connected)".into();
    };

    // Build helix query params from tool args
    let mut params = serde_json::Map::new();
    if let Some(sibling) = args.get("sibling").and_then(serde_json::Value::as_str) {
        params.insert("sibling".into(), serde_json::json!(sibling));
    }
    if let Some(strand) = args.get("strand").and_then(serde_json::Value::as_str) {
        params.insert("strands".into(), serde_json::json!([strand]));
    }
    if let Some(sig) = args
        .get("min_significance")
        .and_then(serde_json::Value::as_f64)
    {
        params.insert("significance_min".into(), serde_json::json!(sig));
    }
    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(5)
        .min(10);
    params.insert("limit".into(), serde_json::json!(limit));

    let request = JsonRpcRequest::tools_call(
        0,
        "soulTools",
        serde_json::json!({
            "action": "helix",
            "params": serde_json::Value::Object(params),
        }),
    );

    match pool.call("soul", &request).await {
        Ok(resp) if !resp.is_error() => {
            let result = resp.result.clone().unwrap_or(serde_json::Value::Null);
            format_helix_response(&result)
        }
        Ok(resp) => {
            let err = resp.error.as_ref().map_or("unknown error", |e| &e.message);
            format!("Helix query failed: {err}")
        }
        Err(e) => format!("Helix query error: {e}"),
    }
}

/// Format helix query results into readable text for the agent.
fn format_helix_response(result: &serde_json::Value) -> String {
    let entries = result
        .get("content")
        .or_else(|| result.get("entries"))
        .and_then(serde_json::Value::as_array);

    let Some(entries) = entries else {
        // Fallback: return raw JSON (truncated)
        let raw = serde_json::to_string_pretty(result).unwrap_or_default();
        let preview: String = raw.chars().take(2000).collect();
        return format!("Helix query result:\n{preview}");
    };

    let mut output = format!("Found {} helix entries:\n\n", entries.len());
    for (i, entry) in entries.iter().take(10).enumerate() {
        let title = entry
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Untitled");
        let sig = entry
            .get("significance")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let strands = entry
            .get("strands")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let _ = writeln!(
            output,
            "{}. **{title}** (sig: {sig:.1}, strands: {strands})",
            i + 1,
        );
    }
    output
}

/// Read a file from shared/ or helix vault (read-only, path-validated).
fn read_file_tool(args: &serde_json::Value, data_dir: Option<&Path>) -> String {
    let Some(path_str) = args.get("path").and_then(serde_json::Value::as_str) else {
        return "Error: missing 'path' arg. Usage: {\"path\": \"shared/bulletin/research-feed.md\"}".into();
    };

    let Some(base) = data_dir else {
        return "Error: file read unavailable (data directory not configured)".into();
    };

    // Security: only allow paths under shared/ or helix vault
    let resolved = if path_str.starts_with("shared/") {
        base.join(path_str)
    } else if path_str.starts_with("helix/")
        || path_str.starts_with("~/lightarchitects/soul/helix/")
    {
        let helix_root = paths::helix_root_or_fallback();
        let clean = path_str
            .strip_prefix("~/lightarchitects/soul/")
            .or_else(|| path_str.strip_prefix("helix/"))
            .unwrap_or(path_str);
        helix_root.join(clean)
    } else {
        return format!("Error: path '{path_str}' not allowed. Only shared/ and helix/ paths.");
    };

    // Security: canonicalize to resolve symlinks, then verify the result stays within
    // the allowed base directory. `contains("..")` can be bypassed via symlinks that
    // point outside the allowed prefix without any ".." in the path string.
    let Ok(canonical) = std::fs::canonicalize(&resolved) else {
        return format!("Error: file not found at '{path_str}'");
    };

    let allowed_base = if path_str.starts_with("shared/") {
        std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf())
    } else {
        let helix = paths::helix_root_or_fallback();
        std::fs::canonicalize(&helix).unwrap_or(helix)
    };

    if !canonical.starts_with(&allowed_base) {
        tracing::warn!(path = %path_str, "[security] Path escape attempt rejected in read_file_tool");
        return "Error: path traversal not allowed".into();
    }

    match std::fs::read_to_string(&canonical) {
        Ok(content) => {
            if content.len() > READ_FILE_MAX_BYTES {
                let preview: String = content.chars().take(READ_FILE_MAX_BYTES).collect();
                format!("{preview}\n\n... (truncated at {READ_FILE_MAX_BYTES} bytes)")
            } else {
                content
            }
        }
        Err(_) => format!("Error: file not found at '{path_str}'"),
    }
}

/// Maximum bytes allowed per `write_file` call.
const WRITE_FILE_MAX_BYTES: usize = 32_768;

/// Allowed vault write prefixes (relative to `~/lightarchitects/soul/helix/`).
const WRITE_ALLOWED_PREFIXES: &[&str] = &[
    "shared/thinktank/",
    "shared/research/summaries/",
    "shared/research/papers/",
    "shared/devotionals/reflections/",
];

/// Sibling names that can have `{sibling}/journal/` write paths.
const SIBLING_NAMES: &[&str] = &["eva", "corso", "quantum", "seraph", "ayin", "claude"];

/// Write a file to the SOUL vault (security-scoped).
///
/// Validates that the path falls under allowed directories, prevents path
/// traversal, and creates parent directories automatically.
fn write_file_tool(args: &serde_json::Value, _data_dir: Option<&Path>) -> String {
    let Some(path_str) = args.get("path").and_then(serde_json::Value::as_str) else {
        return "Error: missing 'path' arg. Usage: {\"path\": \"shared/thinktank/my-analysis.md\", \"content\": \"...\"}".into();
    };
    let Some(content) = args.get("content").and_then(serde_json::Value::as_str) else {
        return "Error: missing 'content' arg.".into();
    };

    if content.len() > WRITE_FILE_MAX_BYTES {
        return format!(
            "Error: content too large ({} bytes). Maximum: {WRITE_FILE_MAX_BYTES} bytes.",
            content.len()
        );
    }

    // Normalize path — strip absolute prefixes, resolve to helix-relative
    let clean = path_str
        .strip_prefix("/home/khadas/.soul/helix/")
        .or_else(|| path_str.strip_prefix("~/lightarchitects/soul/helix/"))
        .or_else(|| path_str.strip_prefix("helix/"))
        .unwrap_or(path_str);

    // Reject path traversal
    if clean.contains("..") {
        tracing::warn!(path = %path_str, "[security] Path traversal rejected in write_file_tool");
        return "Error: path traversal not allowed.".into();
    }

    // Validate against allowed prefixes
    let is_shared_allowed = WRITE_ALLOWED_PREFIXES.iter().any(|p| clean.starts_with(p));
    let is_journal_allowed = SIBLING_NAMES
        .iter()
        .any(|s| clean.starts_with(&format!("{s}/journal/")));

    if !is_shared_allowed && !is_journal_allowed {
        return format!(
            "Error: path '{clean}' not in allowed directories. \
             Allowed: shared/thinktank/, shared/research/summaries/, \
             shared/research/papers/, shared/devotionals/reflections/, \
             {{sibling}}/journal/"
        );
    }

    // Must end with .md
    if !clean.to_ascii_lowercase().ends_with(".md") {
        return "Error: only .md files can be written.".into();
    }

    let full_path = paths::helix_root_or_fallback().join(clean);

    // Create parent directory
    if let Some(parent) = full_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return format!("Error: failed to create directory: {e}");
        }
    }

    // Security: canonicalize the parent after creation to resolve any symlinks,
    // then verify the canonical path stays within the helix vault root.
    // `contains("..")` above is bypassed by symlinks — canonicalize closes that gap.
    let helix_base = paths::helix_root_or_fallback();
    let canonical_base = std::fs::canonicalize(&helix_base).unwrap_or(helix_base);
    let Some(canonical_parent) = full_path
        .parent()
        .and_then(|p| std::fs::canonicalize(p).ok())
    else {
        return "Error: failed to resolve write path".into();
    };
    if !canonical_parent.starts_with(&canonical_base) {
        tracing::warn!(
            path = %path_str,
            "[security] Path escape attempt rejected in write_file_tool"
        );
        return "Error: path traversal not allowed.".into();
    }

    // Write (create or overwrite)
    match std::fs::write(&full_path, content) {
        Ok(()) => {
            tracing::info!(
                path = %full_path.display(),
                bytes = content.len(),
                "Vault file written"
            );
            format!("OK: saved {} bytes to {clean}", content.len())
        }
        Err(e) => {
            tracing::error!(path = %full_path.display(), error = %e, "write_file failed");
            format!("Error: failed to write file: {e}")
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    /// Helper: create a JSON args value with a single "path" key.
    fn path_arg(path: &str) -> serde_json::Value {
        serde_json::json!({ "path": path })
    }

    /// Task 3.2 — symlink escape: a symlink inside shared/ pointing outside the sandbox
    /// must be rejected regardless of the path string not containing "..".
    #[test]
    fn test_read_file_symlink_escape_rejected() {
        let sandbox = TempDir::new().expect("temp dir");
        let shared_dir = sandbox.path().join("shared");
        fs::create_dir_all(&shared_dir).expect("shared dir");

        // Create a symlink shared/evil -> /etc/passwd (no ".." in path string)
        let symlink_path = shared_dir.join("evil.txt");
        symlink("/etc/passwd", &symlink_path).expect("symlink");

        let result = read_file_tool(&path_arg("shared/evil.txt"), Some(sandbox.path()));

        assert!(
            result.contains("Error"),
            "Expected rejection but got: {result}"
        );
        assert!(
            !result.contains("root") && !result.contains("daemon"),
            "Symlink escape: /etc/passwd content leaked: {result}"
        );
    }

    /// Task 3.3 — valid path: a legitimate file within the shared/ sandbox reads successfully.
    #[test]
    fn test_read_file_valid_path_succeeds() {
        let sandbox = TempDir::new().expect("temp dir");
        let shared_dir = sandbox.path().join("shared");
        fs::create_dir_all(&shared_dir).expect("shared dir");

        let test_file = shared_dir.join("bulletin.md");
        fs::write(&test_file, "# Test Bulletin\nHello from the sandbox.").expect("write");

        let result = read_file_tool(&path_arg("shared/bulletin.md"), Some(sandbox.path()));

        assert_eq!(result, "# Test Bulletin\nHello from the sandbox.");
    }
}
