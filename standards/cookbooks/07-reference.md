<!-- uuid: 38ad3ef7-fe3e-4a31-bb1e-307501757e74 -->

---
id: "c5330cea-c1932fe5"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# Cookbook 07: Reference - Error Handling & Testing

**Purpose**: Error handling patterns and testing strategies for Rust MCP servers
**Key Concept**: Unified error types, comprehensive testing, fidelity validation
**Audience**: All MCP server developers

---

## Error Types

### Unified Error Enum

```rust
// src/error.rs

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // Input/Validation errors
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    // Tool errors
    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    #[error("Tool execution failed: {0}")]
    ToolExecution(String),

    // Provider errors
    #[error("Provider unavailable: {0}")]
    ProviderUnavailable(String),

    #[error("Provider request failed: {0}")]
    ProviderRequest(String),

    #[error("Rate limited")]
    RateLimited,

    // Workflow errors
    #[error("Workflow error: {0}")]
    Workflow(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    // Security errors
    #[error("Blocked: {0}")]
    Blocked(String),

    #[error("Review required: {0}")]
    ReviewRequired(String),

    // IO errors
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    // Serialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // Internal errors
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl Error {
    /// Map to JSON-RPC error code
    pub fn code(&self) -> i32 {
        match self {
            Error::InvalidInput(_) | Error::MissingParameter(_) => -32602,
            Error::UnknownTool(_) => -32601,
            Error::Blocked(_) => -32000,
            Error::ReviewRequired(_) => -32001,
            Error::RateLimited => -32002,
            Error::Timeout(_) => -32003,
            _ => -32000,
        }
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::ProviderUnavailable(_) |
            Error::RateLimited |
            Error::Timeout(_)
        )
    }
}
```

### Error Conversion

```rust
// Convert orchestrator-specific errors
impl From<crate::orchestrators::your_tool::YourToolError> for Error {
    fn from(e: crate::orchestrators::your_tool::YourToolError) -> Self {
        Error::ToolExecution(e.to_string())
    }
}

// Convert provider errors
impl From<crate::providers::ProviderError> for Error {
    fn from(e: crate::providers::ProviderError) -> Self {
        match e {
            crate::providers::ProviderError::Unavailable(msg) => Error::ProviderUnavailable(msg),
            crate::providers::ProviderError::RateLimited => Error::RateLimited,
            crate::providers::ProviderError::RequestFailed(msg) => Error::ProviderRequest(msg),
            crate::providers::ProviderError::InvalidResponse(msg) => Error::Internal(msg),
        }
    }
}
```

---

## Error Handling Patterns

### Result Type Alias

```rust
// src/lib.rs
pub type Result<T> = std::result::Result<T, Error>;
```

### Propagation with Context

```rust
use anyhow::Context;

async fn complex_operation() -> Result<Output> {
    let data = read_file(&path)
        .context("Failed to read input file")?;

    let parsed = parse_data(&data)
        .context("Failed to parse data")?;

    let result = process(&parsed)
        .context("Failed to process data")?;

    Ok(result)
}
```

### Error Mapping

```rust
async fn fetch_external() -> Result<Data> {
    reqwest::get(url)
        .await
        .map_err(|e| Error::ProviderRequest(format!("HTTP request failed: {}", e)))?
        .json()
        .await
        .map_err(|e| Error::Internal(format!("Failed to parse response: {}", e)))
}
```

### Graceful Fallback

```rust
async fn get_data_with_fallback() -> Result<Data> {
    // Try primary source
    match primary_source().await {
        Ok(data) => return Ok(data),
        Err(e) => {
            tracing::warn!("Primary source failed: {}, trying fallback", e);
        }
    }

    // Try fallback
    fallback_source()
        .await
        .map_err(|e| Error::Internal(format!("Both sources failed: {}", e)))
}
```

---

## Testing Patterns

### Unit Test Structure

```rust
// tests/your_tool_test.rs

use your_server::orchestrators::your_tool::{self, YourToolParams, YourToolResult};

#[tokio::test]
async fn test_basic_execution() {
    let params = YourToolParams {
        query: "test query".to_string(),
        limit: 10,
    };

    let result = your_tool::execute(params).await;

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(!result.items.is_empty());
}

#[tokio::test]
async fn test_empty_query() {
    let params = YourToolParams {
        query: "".to_string(),
        limit: 10,
    };

    let result = your_tool::execute(params).await;

    assert!(matches!(result, Err(Error::InvalidInput(_))));
}

#[tokio::test]
async fn test_limit_enforcement() {
    let params = YourToolParams {
        query: "test".to_string(),
        limit: 5,
    };

    let result = your_tool::execute(params).await.unwrap();

    assert!(result.items.len() <= 5);
}
```

### Mock Providers

```rust
// tests/mocks.rs

use async_trait::async_trait;
use your_server::providers::{AIProvider, ProviderRequest, ProviderResponse, ProviderError};

pub struct MockProvider {
    pub should_fail: bool,
    pub response: String,
}

#[async_trait]
impl AIProvider for MockProvider {
    fn name(&self) -> &'static str { "mock" }
    fn tier(&self) -> u8 { 0 }

    async fn is_available(&self) -> bool { true }

    async fn generate(&self, _request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        if self.should_fail {
            Err(ProviderError::RequestFailed("Mock failure".to_string()))
        } else {
            Ok(ProviderResponse {
                content: self.response.clone(),
                model: "mock".to_string(),
                tokens_used: 100,
                provider: "mock".to_string(),
            })
        }
    }
}
```

### Integration Tests

```rust
// tests/integration_test.rs

use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};

#[test]
fn test_mcp_tools_list() {
    let mut child = Command::new("./target/release/your-server")
        .args(["mcp-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    // Send request
    let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    writeln!(stdin, "{}", request).unwrap();
    stdin.flush().unwrap();

    // Read response
    let mut lines = reader.lines();
    let response = lines.next().unwrap().unwrap();

    // Parse and verify
    let json: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(json["result"]["tools"].is_array());
    assert!(!json["result"]["tools"].as_array().unwrap().is_empty());

    // Cleanup
    drop(stdin);
    child.kill().ok();
}

#[test]
fn test_mcp_tool_call() {
    let mut child = Command::new("./target/release/your-server")
        .args(["mcp-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    // Send tool call
    let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"your_tool","arguments":{"query":"test"}}}"#;
    writeln!(stdin, "{}", request).unwrap();
    stdin.flush().unwrap();

    // Read response
    let mut lines = reader.lines();
    let response = lines.next().unwrap().unwrap();

    // Verify success
    let json: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(json["error"].is_null(), "Expected success, got error: {:?}", json["error"]);

    // Cleanup
    drop(stdin);
    child.kill().ok();
}
```

### Hook Tests

```rust
// tests/hook_test.rs

use your_server::hooks::{HookRegistry, HookContext, HookResult};
use your_server::hooks::builtin::InputValidationHook;

#[tokio::test]
async fn test_input_validation_hook() {
    let hook = InputValidationHook;

    // Valid input
    let ctx = HookContext::new("test_tool", serde_json::json!({
        "query": "valid query"
    }));

    let result = hook.execute(ctx).await;
    assert!(matches!(result, HookResult::Continue(_)));

    // Invalid input (injection attempt)
    let ctx = HookContext::new("test_tool", serde_json::json!({
        "query": "'; DROP TABLE users; --"
    }));

    let result = hook.execute(ctx).await;
    // Depending on your validation, this might block
}

#[tokio::test]
async fn test_hook_chain_execution() {
    let registry = HookRegistry::new();

    let ctx = HookContext::new("test_tool", serde_json::json!({
        "query": "test"
    }));

    let result = registry.run_pre_hooks(ctx).await;

    match result {
        HookResult::Continue(ctx) => {
            // Verify hooks added metadata
            assert!(ctx.get_metadata::<bool>("validated").unwrap_or(false));
        }
        HookResult::Block { reason, .. } => {
            panic!("Unexpected block: {}", reason);
        }
        _ => panic!("Unexpected result"),
    }
}
```

---

## Fidelity Testing (Persona Validation)

For servers with persona/personality systems:

```rust
// tests/fidelity_test.rs

use your_server::persona::Persona;

/// Test that generated responses match expected persona traits
#[tokio::test]
async fn test_persona_fidelity() {
    let persona = Persona::load().unwrap();

    // Test prompts covering different scenarios
    let test_cases = vec![
        ("Hello!", "greeting"),
        ("I'm frustrated with this bug", "support"),
        ("What do you think about this code?", "review"),
    ];

    for (input, category) in test_cases {
        let response = persona.generate_response(input, "auto").await.unwrap();

        // Check signature phrases are present
        let signature_count = persona.signature_phrases()
            .iter()
            .filter(|phrase| response.contains(*phrase))
            .count();

        assert!(signature_count >= 1,
            "Response for '{}' should contain at least 1 signature phrase: {}",
            category, response
        );

        // Check anti-patterns are absent
        for anti_pattern in persona.anti_patterns() {
            assert!(!response.contains(anti_pattern),
                "Response for '{}' contains anti-pattern '{}': {}",
                category, anti_pattern, response
            );
        }
    }
}

/// Compare generated response to ground truth samples
#[tokio::test]
async fn test_response_similarity() {
    let persona = Persona::load().unwrap();
    let samples = load_ground_truth_samples();

    for sample in samples {
        let generated = persona.generate_response(&sample.prompt, "auto").await.unwrap();

        // Calculate similarity (could use embedding similarity in production)
        let similarity = jaccard_similarity(&generated, &sample.expected);

        assert!(similarity > 0.3,
            "Generated response too different from expected.\nPrompt: {}\nGenerated: {}\nExpected: {}\nSimilarity: {}",
            sample.prompt, generated, sample.expected, similarity
        );
    }
}

fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<_> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<_> = b.split_whitespace().collect();

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}
```

---

## Test Configuration

```toml
# Cargo.toml

[dev-dependencies]
tokio-test = "0.4"
mockall = "0.11"
tempfile = "3"
assert_matches = "1.5"

[[test]]
name = "integration"
path = "tests/integration_test.rs"
harness = true

[[test]]
name = "fidelity"
path = "tests/fidelity_test.rs"
harness = true
```

---

## CI/CD Testing

```yaml
# .github/workflows/test.yml

name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true

      - name: Run unit tests
        run: cargo test --lib

      - name: Run integration tests
        run: |
          cargo build --release
          cargo test --test integration

      - name: Run fidelity tests
        run: cargo test --test fidelity
        if: github.ref == 'refs/heads/main'
```

---

## Debugging Tips

### Enable Logging

```bash
# Run with debug logging
RUST_LOG=debug cargo run -- mcp-server

# Filter to specific modules
RUST_LOG=your_server::orchestrators=debug cargo run -- mcp-server
```

### Test Single Tool via CLI

```bash
# Build release
cargo build --release

# Test tool directly
./target/release/your-server your-tool "test input"
```

### Interactive MCP Testing

```bash
# Start server and interact manually
./target/release/your-server mcp-server

# Then type JSON-RPC requests:
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

---

## Cookbook Index

| Cookbook | Topic |
|----------|-------|
| [00-getting-started.md](./00-getting-started.md) | Project structure and setup |
| [01-foundations.md](./01-foundations.md) | Hooks and utilities |
| [02-orchestrator.md](./02-orchestrator.md) | Building MCP tools |
| [03-security.md](./03-security.md) | Security patterns |
| [04-provider.md](./04-provider.md) | AI provider integration |
| [05-mcp.md](./05-mcp.md) | MCP server implementation |
| [06-workflow.md](./06-workflow.md) | Multi-tool orchestration |
| **07-reference.md** | Error handling and testing |

---

*Platform-agnostic error handling and testing patterns for any Rust MCP server*

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
