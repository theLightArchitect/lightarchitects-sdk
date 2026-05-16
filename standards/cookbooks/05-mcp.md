<!-- uuid: 5c76aea5-5e2e-4ad5-9dab-89405406d54f -->

---
id: "54bfb49e-a642a4bd"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# Cookbook 05: MCP Server Implementation

**Purpose**: Building JSON-RPC over stdio MCP servers in Rust
**Protocol**: Model Context Protocol (JSON-RPC 2.0)
**Transport**: stdin/stdout (stdio)

---

## MCP Protocol Overview

```
┌─────────────────┐                     ┌─────────────────┐
│  Claude Code    │  ← stdin/stdout →   │  MCP Server     │
│  (Client)       │                     │  (Your Code)    │
└─────────────────┘                     └─────────────────┘

Request (Client → Server):
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}

Response (Server → Client):
{"jsonrpc":"2.0","id":1,"result":{"tools":[...]}}
```

---

## Core MCP Server

```rust
// src/mcp.rs

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

pub async fn run_mcp_server() -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(request) => handle_request(request).await,
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {}", e),
                    data: None,
                }),
            },
        };

        let response_json = serde_json::to_string(&response)?;
        stdout.write_all(response_json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn handle_request(request: JsonRpcRequest) -> JsonRpcResponse {
    let result = match request.method.as_str() {
        "initialize" => handle_initialize(),
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tools_call(request.params).await,
        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {}", request.method),
            data: None,
        }),
    };

    match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(value),
            error: None,
        },
        Err(error) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(error),
        },
    }
}
```

---

## Tool Registration

```rust
// src/mcp.rs (continued)

use crate::orchestrators;

#[derive(Debug, Serialize)]
struct ToolDefinition {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

fn handle_initialize() -> Result<serde_json::Value, JsonRpcError> {
    Ok(serde_json::json!({
        "protocolVersion": "2024-11-05",
        "serverInfo": {
            "name": "your-mcp-server",
            "version": env!("CARGO_PKG_VERSION")
        },
        "capabilities": {
            "tools": {}
        }
    }))
}

fn handle_tools_list() -> Result<serde_json::Value, JsonRpcError> {
    let tools = get_tools();
    Ok(serde_json::json!({ "tools": tools }))
}

fn get_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "your_tool".to_string(),
            description: "Description of what your tool does".to_string(),
            input_schema: orchestrators::your_tool::schema(),
        },
        ToolDefinition {
            name: "another_tool".to_string(),
            description: "Another tool description".to_string(),
            input_schema: orchestrators::another_tool::schema(),
        },
        // Add more tools here
    ]
}
```

---

## Tool Call Handler

```rust
// src/mcp.rs (continued)

async fn handle_tools_call(params: serde_json::Value) -> Result<serde_json::Value, JsonRpcError> {
    let name = params.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing 'name' parameter".to_string(),
            data: None,
        })?;

    let arguments = params.get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let result = match name {
        "your_tool" => {
            let params: orchestrators::your_tool::YourToolParams =
                serde_json::from_value(arguments)
                    .map_err(|e| JsonRpcError {
                        code: -32602,
                        message: format!("Invalid parameters: {}", e),
                        data: None,
                    })?;

            orchestrators::your_tool::execute(params)
                .await
                .map_err(|e| JsonRpcError {
                    code: -32000,
                    message: e.to_string(),
                    data: None,
                })?
        }
        "another_tool" => {
            let params: orchestrators::another_tool::AnotherToolParams =
                serde_json::from_value(arguments)
                    .map_err(|e| JsonRpcError {
                        code: -32602,
                        message: format!("Invalid parameters: {}", e),
                        data: None,
                    })?;

            orchestrators::another_tool::execute(params)
                .await
                .map_err(|e| JsonRpcError {
                    code: -32000,
                    message: e.to_string(),
                    data: None,
                })?
        }
        _ => {
            return Err(JsonRpcError {
                code: -32602,
                message: format!("Unknown tool: {}", name),
                data: None,
            });
        }
    };

    // Format as MCP tool result
    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&result).unwrap_or_default()
        }]
    }))
}
```

---

## Hook Integration

```rust
// src/mcp.rs - with hooks

use crate::hooks::{HookRegistry, HookContext, HookResult};

async fn handle_tools_call_with_hooks(
    params: serde_json::Value,
    hook_registry: &HookRegistry,
) -> Result<serde_json::Value, JsonRpcError> {
    let name = params.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing 'name' parameter".to_string(),
            data: None,
        })?;

    let arguments = params.get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    // Build hook context
    let ctx = HookContext::new(name, arguments.clone());

    // Run pre-hooks
    let ctx = match hook_registry.run_pre_hooks(ctx).await {
        HookResult::Continue(ctx) => ctx,
        HookResult::Block { reason, .. } => {
            return Err(JsonRpcError {
                code: -32000,
                message: format!("Blocked: {}", reason),
                data: None,
            });
        }
        HookResult::Skip { cached_result, .. } => {
            return Ok(serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&cached_result).unwrap_or_default()
                }]
            }));
        }
        HookResult::RequireReview { review_prompt, .. } => {
            return Err(JsonRpcError {
                code: -32001,
                message: format!("Review required: {}", review_prompt),
                data: None,
            });
        }
    };

    // Execute tool
    let result = execute_tool(name, ctx.params.clone()).await?;

    // Run post-hooks
    let mut post_ctx = ctx;
    post_ctx.result = Some(result.clone());

    let final_result = match hook_registry.run_post_hooks(post_ctx).await {
        HookResult::Continue(ctx) => ctx.result.unwrap_or(result),
        _ => result,
    };

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&final_result).unwrap_or_default()
        }]
    }))
}

async fn execute_tool(name: &str, args: serde_json::Value) -> Result<serde_json::Value, JsonRpcError> {
    match name {
        "your_tool" => {
            let params: orchestrators::your_tool::YourToolParams =
                serde_json::from_value(args)
                    .map_err(|e| JsonRpcError {
                        code: -32602,
                        message: format!("Invalid parameters: {}", e),
                        data: None,
                    })?;

            let result = orchestrators::your_tool::execute(params)
                .await
                .map_err(|e| JsonRpcError {
                    code: -32000,
                    message: e.to_string(),
                    data: None,
                })?;

            serde_json::to_value(result)
                .map_err(|e| JsonRpcError {
                    code: -32000,
                    message: format!("Serialization error: {}", e),
                    data: None,
                })
        }
        _ => Err(JsonRpcError {
            code: -32602,
            message: format!("Unknown tool: {}", name),
            data: None,
        }),
    }
}
```

---

## Schema Generation

```rust
// src/orchestrators/your_tool.rs

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query"
            },
            "limit": {
                "type": "integer",
                "default": 10,
                "minimum": 1,
                "maximum": 100,
                "description": "Maximum results to return"
            },
            "include_metadata": {
                "type": "boolean",
                "default": false,
                "description": "Include result metadata"
            },
            "filters": {
                "type": "array",
                "items": {
                    "type": "string"
                },
                "description": "Filter criteria"
            },
            "depth": {
                "type": "string",
                "enum": ["quick", "standard", "deep"],
                "default": "standard",
                "description": "Analysis depth"
            }
        },
        "required": ["query"]
    })
}
```

---

## Error Codes

| Code | Meaning |
|------|---------|
| -32700 | Parse error |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |
| -32000 | Tool execution error |
| -32001 | Review required |

---

## Claude Code Configuration

Add to `~/.config/claude-code/config.json`:

```json
{
  "mcpServers": {
    "your-server": {
      "command": "/absolute/path/to/your-server",
      "args": ["mcp-server"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

---

## Testing the MCP Server

```bash
# Test tools/list
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | ./your-server mcp-server

# Test tool call
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"your_tool","arguments":{"query":"test"}}}' | ./your-server mcp-server
```

---

## Best Practices

1. **Always Flush stdout**: Ensure responses are immediately visible

2. **Handle EOF Gracefully**: Exit cleanly when stdin closes

3. **Validate All Input**: Never trust incoming JSON

4. **Use Descriptive Schemas**: Help Claude understand parameters

5. **Return Structured Results**: Use consistent result formats

6. **Log to stderr**: stdout is for MCP protocol only

7. **Handle Timeouts**: Long operations should have timeouts

---

## Next Steps

- **[06-workflow.md](./06-workflow.md)** - Multi-tool orchestration
- **[01-foundations.md](./01-foundations.md)** - Hook integration

---

*Platform-agnostic MCP server patterns for any Rust implementation*

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
- [[user/standards/canon/builders-cookbook|Builders Cookbook]]
