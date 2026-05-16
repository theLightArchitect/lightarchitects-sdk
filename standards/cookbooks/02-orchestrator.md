<!-- uuid: ed17a31d-14dd-43a5-9886-556ce227b316 -->

---
id: "609100b5-5014c51e"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# Cookbook 02: Building MCP Orchestrators (Tools)

**Purpose**: Patterns for implementing MCP tools in Rust
**Key Concept**: Orchestrators are business logic handlers exposed as MCP tools
**Pattern**: Params → Execute → Result with consistent structure

---

## Orchestrator Anatomy

Every orchestrator follows this pattern:

```rust
// src/orchestrators/your_tool.rs

use serde::{Deserialize, Serialize};
use crate::error::Error;

/// Input parameters (validated by serde)
#[derive(Debug, Deserialize)]
pub struct YourToolParams {
    pub required_field: String,
    #[serde(default)]
    pub optional_field: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 10 }

/// Output result
#[derive(Debug, Serialize)]
pub struct YourToolResult {
    pub data: Vec<Item>,
    pub metadata: ResultMetadata,
}

#[derive(Debug, Serialize)]
pub struct ResultMetadata {
    pub count: usize,
    pub execution_time_ms: u64,
}

/// Main execution function
pub async fn execute(params: YourToolParams) -> Result<YourToolResult, Error> {
    let start = std::time::Instant::now();

    // Your business logic here
    let data = process_data(&params.required_field).await?;

    Ok(YourToolResult {
        data: data.clone(),
        metadata: ResultMetadata {
            count: data.len(),
            execution_time_ms: start.elapsed().as_millis() as u64,
        },
    })
}

/// Schema for MCP tool registration
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "required_field": {
                "type": "string",
                "description": "Description of required field"
            },
            "optional_field": {
                "type": "string",
                "description": "Optional field description"
            },
            "limit": {
                "type": "integer",
                "default": 10,
                "description": "Maximum results to return"
            }
        },
        "required": ["required_field"]
    })
}
```

---

## Module Organization

```rust
// src/orchestrators/mod.rs

mod your_tool;
mod another_tool;

// Re-export public interfaces
pub use your_tool::{YourToolParams, YourToolResult, execute as execute_your_tool, schema as your_tool_schema};
pub use another_tool::{AnotherToolParams, AnotherToolResult, execute as execute_another_tool, schema as another_tool_schema};
```

---

## Parameter Validation Patterns

### Required vs Optional Fields

```rust
#[derive(Debug, Deserialize)]
pub struct Params {
    // Required - will error if missing
    pub query: String,

    // Optional with None default
    #[serde(default)]
    pub filter: Option<String>,

    // Optional with specific default
    #[serde(default = "default_limit")]
    pub limit: usize,

    // Optional with inline default
    #[serde(default)]
    pub include_metadata: bool, // defaults to false
}

fn default_limit() -> usize { 100 }
```

### Enum Parameters

```rust
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisDepth {
    Quick,
    #[default]
    Standard,
    Deep,
    Full,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeParams {
    pub input: String,
    #[serde(default)]
    pub depth: AnalysisDepth,
}
```

### Validation in Execute

```rust
pub async fn execute(params: Params) -> Result<Result, Error> {
    // Validate constraints
    if params.query.is_empty() {
        return Err(Error::InvalidInput("query cannot be empty".to_string()));
    }

    if params.limit > 1000 {
        return Err(Error::InvalidInput("limit cannot exceed 1000".to_string()));
    }

    // Business logic...
}
```

---

## Result Patterns

### Simple Result

```rust
#[derive(Debug, Serialize)]
pub struct SimpleResult {
    pub success: bool,
    pub message: String,
}
```

### Collection Result

```rust
#[derive(Debug, Serialize)]
pub struct CollectionResult<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub has_more: bool,
}
```

### Scored/Ranked Result

```rust
#[derive(Debug, Serialize)]
pub struct ScoredItem {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub confidence: Confidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    Definitive,  // 90%+
    Strong,      // 70-89%
    Moderate,    // 50-69%
    Low,         // <50%
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub matches: Vec<ScoredItem>,
    pub query_time_ms: u64,
}
```

### Result with Persona/Commentary

```rust
#[derive(Debug, Serialize)]
pub struct ResultWithPersona<T> {
    pub data: T,
    pub persona: PersonaOutput,
}

#[derive(Debug, Serialize)]
pub struct PersonaOutput {
    pub commentary: String,
    pub suggestions: Vec<String>,
    pub mood: String,
}
```

---

## Async Patterns

### Parallel Execution

```rust
use tokio::join;

pub async fn execute(params: Params) -> Result<Result, Error> {
    // Run searches in parallel
    let (pattern_result, case_result, jira_result) = join!(
        search_patterns(&params.query),
        search_cases(&params.query),
        search_jira(&params.query),
    );

    // Combine results
    Ok(Result {
        patterns: pattern_result?,
        cases: case_result?,
        jira_issues: jira_result?,
    })
}
```

### Timeout Handling

```rust
use tokio::time::{timeout, Duration};

pub async fn execute(params: Params) -> Result<Result, Error> {
    let result = timeout(
        Duration::from_secs(30),
        expensive_operation(&params),
    ).await
        .map_err(|_| Error::Timeout("Operation timed out after 30 seconds".to_string()))??;

    Ok(result)
}
```

### Retry Pattern

```rust
pub async fn execute_with_retry<T, F, Fut>(
    max_retries: usize,
    mut operation: F,
) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, Error>>,
{
    let mut last_error = None;

    for attempt in 0..max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() => {
                last_error = Some(e);
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt as u32))).await;
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_error.unwrap_or(Error::MaxRetriesExceeded))
}
```

---

## Error Handling

### Orchestrator-Specific Errors

```rust
// src/orchestrators/your_tool.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum YourToolError {
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("No results found for: {0}")]
    NoResults(String),

    #[error("External service error: {0}")]
    ExternalService(String),
}

impl From<YourToolError> for crate::error::Error {
    fn from(e: YourToolError) -> Self {
        crate::error::Error::Orchestrator(e.to_string())
    }
}
```

### Graceful Degradation

```rust
pub async fn execute(params: Params) -> Result<Result, Error> {
    // Try primary source
    let primary_result = match primary_search(&params.query).await {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!("Primary search failed: {}, falling back", e);
            None
        }
    };

    // Fallback if primary fails
    let results = match primary_result {
        Some(r) => r,
        None => fallback_search(&params.query).await?,
    };

    Ok(results)
}
```

---

## Testing Orchestrators

```rust
// tests/your_tool_test.rs

#[tokio::test]
async fn test_your_tool_basic() {
    let params = YourToolParams {
        required_field: "test".to_string(),
        optional_field: None,
        limit: 5,
    };

    let result = your_tool::execute(params).await.unwrap();

    assert!(!result.data.is_empty());
    assert!(result.metadata.count <= 5);
}

#[tokio::test]
async fn test_your_tool_validation_error() {
    let params = YourToolParams {
        required_field: "".to_string(), // Invalid
        optional_field: None,
        limit: 5,
    };

    let result = your_tool::execute(params).await;

    assert!(matches!(result, Err(Error::InvalidInput(_))));
}
```

---

## Best Practices

1. **Keep Orchestrators Focused**: One tool = one capability

2. **Consistent Naming**:
   - Params: `{ToolName}Params`
   - Result: `{ToolName}Result`
   - Function: `execute()`
   - Schema: `schema()`

3. **Validate Early**: Check params at the start of `execute()`

4. **Include Metadata**: Execution time, counts, pagination info

5. **Document Schema**: Provide clear descriptions in `schema()`

6. **Handle Timeouts**: External calls should have timeouts

7. **Log Appropriately**: Use `tracing` for debug info

---

## Next Steps

- **[03-security.md](./03-security.md)** - Security patterns
- **[05-mcp.md](./05-mcp.md)** - Registering tools with MCP server

---

*Platform-agnostic orchestrator patterns for any Rust MCP server*

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
- [[user/standards/canon/builders-cookbook|Builders Cookbook]]
